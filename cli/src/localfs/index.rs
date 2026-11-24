//! Bounded lexical declaration/import inventory. Not a semantic resolver.
use super::search::{MAX_WALK_DEPTH, MAX_WALK_ENTRIES};
use super::{LocalReadError, LocalReader, Result, bounded_bytes, open_beneath, validated_segments};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub(crate) mod cache;
use std::{collections::BTreeSet, path::Path};

const FILE_CAP: u64 = 256 * 1024;
const ITEM_CAP: usize = 200;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIndex {
    path: String,
    language: &'static str,
    symbols: Vec<Symbol>,
    imports: Vec<Import>,
    truncated: bool,
    analysis: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Symbol {
    name: String,
    kind: String,
    line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Import {
    reference: String,
    line: usize,
}

fn identifier(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(text.len());
    let name = &text[..end];
    if name.len() > 128
        || !name
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphabetic() || matches!(b, b'_' | b'$'))
    {
        return None;
    }
    Some(name)
}

fn strip_words<'a>(mut line: &'a str, words: &[&str]) -> &'a str {
    loop {
        let Some((word, rest)) = line.split_once(char::is_whitespace) else {
            return line;
        };
        if !words.contains(&word) {
            return line;
        }
        line = rest.trim_start();
    }
}

fn declaration<'a>(line: &'a str, lang: &str) -> Option<(&'a str, &'static str)> {
    let (line, keywords): (&str, &[(&str, &str)]) = match lang {
        "Rust" => {
            let line = line.strip_prefix("pub ").unwrap_or(line);
            let line = if let Some(rest) = line.strip_prefix("pub(") {
                rest.split_once(')')?.1.trim_start()
            } else {
                line
            };
            (
                strip_words(
                    line.strip_prefix("const fn ")
                        .map_or(line, |_| line.trim_start_matches("const ")),
                    &["async", "unsafe"],
                ),
                &[
                    ("fn ", "function"),
                    ("struct ", "struct"),
                    ("enum ", "enum"),
                    ("trait ", "trait"),
                    ("type ", "type"),
                    ("mod ", "module"),
                    ("const ", "constant"),
                    ("static ", "static"),
                ],
            )
        }
        "Go" => {
            let line = if let Some(rest) = line.strip_prefix("func (") {
                let (_, rest) = rest.split_once(')')?;
                return identifier(rest).map(|name| (name, "method"));
            } else {
                line
            };
            (
                line,
                &[
                    ("func ", "function"),
                    ("type ", "type"),
                    ("const ", "constant"),
                    ("var ", "variable"),
                ],
            )
        }
        "Python" => (
            strip_words(line, &["async"]),
            &[("def ", "function"), ("class ", "class")],
        ),
        "TypeScript" | "JavaScript" => (
            strip_words(line, &["export", "default", "declare", "abstract", "async"]),
            &[
                ("function ", "function"),
                ("class ", "class"),
                ("interface ", "interface"),
                ("type ", "type"),
                ("enum ", "enum"),
                ("const ", "constant"),
                ("let ", "variable"),
            ],
        ),
        _ => return None,
    };
    for &(prefix, kind) in keywords {
        if let Some(rest) = line.strip_prefix(prefix) {
            return identifier(rest).map(|name| (name, kind));
        }
    }
    None
}

// Return only bounded module-like references, never entire source statements,
// URL imports, query strings, credentials or arbitrary string contents.
fn module_reference(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 192
        || raw.starts_with('/')
        || raw.contains("://")
        || !raw.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b':' | b'@' | b'$')
        })
    {
        return None;
    }
    Some(raw)
}

fn quoted_module(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let quote = text.chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }
    module_reference(text[1..].split_once(quote)?.0)
}

fn import_ref<'a>(line: &'a str, lang: &str, go_block: bool) -> Option<&'a str> {
    match lang {
        "Rust" => {
            let line = line.strip_prefix("pub ").unwrap_or(line);
            let rest = line.strip_prefix("use ")?.trim_start();
            // Grouped use trees contribute their shared module prefix only.
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '{' | ';' | '*'))
                .unwrap_or(rest.len());
            module_reference(rest[..end].trim_end_matches("::"))
        }
        "Go" => {
            let rest = if go_block {
                line
            } else {
                line.strip_prefix("import ")?
            };
            let start = rest.find('"')?;
            quoted_module(&rest[start..])
        }
        "Python" => {
            let rest = line
                .strip_prefix("from ")
                .or_else(|| line.strip_prefix("import "))?;
            module_reference(rest.split(|c: char| c.is_whitespace() || c == ',').next()?)
        }
        "TypeScript" | "JavaScript" => {
            let rest = line
                .strip_prefix("import ")
                .or_else(|| line.strip_prefix("export "))?;
            if let Some(module) = quoted_module(rest) {
                return Some(module);
            }
            let (_, tail) = rest.rsplit_once(" from ")?;
            quoted_module(tail)
        }
        _ => None,
    }
}

fn analyze(path: String, lang: &'static str, text: &str) -> SourceIndex {
    let mut result = SourceIndex {
        path,
        language: lang,
        symbols: Vec::new(),
        imports: Vec::new(),
        truncated: false,
        analysis: "lexical_candidates",
    };
    let mut go_block = false;
    let mut block_comment = false;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if block_comment {
            if line.contains("*/") {
                block_comment = false;
            }
            continue;
        }
        if line.starts_with("/*") {
            block_comment = !line.contains("*/");
            continue;
        }
        if line.starts_with("//") || line.starts_with('#') || line.is_empty() {
            continue;
        }
        if lang == "Go" && line == "import (" {
            go_block = true;
            continue;
        }
        if go_block && line == ")" {
            go_block = false;
            continue;
        }
        if let Some((name, kind)) = declaration(line, lang) {
            if result.symbols.len() < ITEM_CAP {
                result.symbols.push(Symbol {
                    name: name.to_owned(),
                    kind: kind.to_owned(),
                    line: index + 1,
                });
            } else {
                result.truncated = true;
            }
        }
        if let Some(reference) = import_ref(line, lang, go_block) {
            if result.imports.len() < ITEM_CAP {
                result.imports.push(Import {
                    reference: reference.to_owned(),
                    line: index + 1,
                });
            } else {
                result.truncated = true;
            }
        }
    }
    result
}

fn source_language(path: &str) -> Option<&'static str> {
    Some(match Path::new(path).extension()?.to_str()? {
        "rs" => "Rust",
        "go" => "Go",
        "py" | "pyi" => "Python",
        "ts" | "tsx" | "mts" | "cts" => "TypeScript",
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript",
        _ => return None,
    })
}

impl LocalReader {
    /// Index a single explicitly requested file without returning source text.
    pub fn source_index(&self, path: &str) -> Result<SourceIndex> {
        let segments = validated_segments(path)?;
        let relative = segments.join("/");
        let lang = source_language(&relative).ok_or(LocalReadError::Denied(
            "source index supports Rust, Go, Python, TypeScript and JavaScript",
        ))?;
        let file = open_beneath(&self.root, &segments)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(LocalReadError::Denied("not a regular source file"));
        }
        let bytes = bounded_bytes(file, meta.len(), FILE_CAP)?;
        if bytes.contains(&0) {
            return Err(LocalReadError::Denied("binary source file"));
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| LocalReadError::InvalidUtf8)?;
        Ok(analyze(relative, lang, text))
    }
}

const WORKSPACE_FILE_CAP: usize = 512;
const WORKSPACE_BYTE_CAP: u64 = 16 * 1024 * 1024;
const WORKSPACE_RESULT_CAP: usize = 200;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanStats {
    scanned_entries: usize,
    indexed_files: usize,
    scanned_bytes: u64,
    skipped_sources: usize,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<cache::CacheStats>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSearch {
    query: String,
    matches: Vec<SymbolMatch>,
    scan: ScanStats,
    results_truncated: bool,
    analysis: &'static str,
}

#[derive(Debug, Serialize)]
struct SymbolMatch {
    path: String,
    language: &'static str,
    name: String,
    kind: String,
    line: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyMap {
    edges: Vec<DependencyEdge>,
    scan: ScanStats,
    results_truncated: bool,
    analysis: &'static str,
}

#[derive(Debug, Serialize)]
struct DependencyEdge {
    source: String,
    reference: String,
    line: usize,
    status: &'static str,
    candidates: Vec<String>,
}

fn result_limit(limit: usize) -> Result<()> {
    if !(1..=WORKSPACE_RESULT_CAP).contains(&limit) {
        return Err(LocalReadError::Denied("result limit must be 1-200"));
    }
    Ok(())
}

// Resolve only lexical JS/TS relative references against indexed paths.
// No external package lookup, filesystem probing, alias configuration, or
// guessed compiler semantics. Ambiguous matches are reported explicitly.
fn relative_candidates(source: &str, reference: &str, files: &BTreeSet<String>) -> Vec<String> {
    if !(reference.starts_with("./") || reference.starts_with("../")) {
        return Vec::new();
    }
    let parent = source.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut components: Vec<&str> = parent.split('/').filter(|s| !s.is_empty()).collect();
    for component in reference.split('/') {
        match component {
            "." => (),
            ".." => {
                if components.pop().is_none() {
                    return Vec::new();
                }
            }
            "" => return Vec::new(),
            value => components.push(value),
        }
    }
    let base = components.join("/");
    if base.is_empty() || validated_segments(&base).is_err() {
        return Vec::new();
    }
    let mut matches = BTreeSet::new();
    if files.contains(&base) && matches!(source_language(&base), Some("JavaScript" | "TypeScript"))
    {
        matches.insert(base.clone());
    }
    if Path::new(&base).extension().is_none() {
        for suffix in ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"] {
            for candidate in [format!("{base}.{suffix}"), format!("{base}/index.{suffix}")] {
                if files.contains(&candidate) {
                    matches.insert(candidate);
                }
            }
        }
    }
    matches.into_iter().collect()
}

impl LocalReader {
    fn scan_workspace_sources(&self, mut visit: impl FnMut(SourceIndex)) -> Result<ScanStats> {
        let mut stats = ScanStats::default();
        let mut cache = cache::ScanCache::start(self);
        for entry in self.walk() {
            let entry =
                entry.map_err(|_| LocalReadError::Denied("source index traversal failed"))?;
            if entry.depth() == 0 {
                continue;
            }
            if stats.scanned_entries >= MAX_WALK_ENTRIES
                || stats.indexed_files >= WORKSPACE_FILE_CAP
                || stats.scanned_bytes >= WORKSPACE_BYTE_CAP
            {
                stats.truncated = true;
                break;
            }
            stats.scanned_entries += 1;
            if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                if entry.depth() == MAX_WALK_DEPTH {
                    stats.truncated = true;
                }
                continue;
            }
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let path = self.relative_name(&entry)?;
            let Some(lang) = source_language(&path) else {
                continue;
            };
            // Keep the response size bounded even with deeply nested paths.
            if path.len() > 1024 {
                stats.skipped_sources += 1;
                stats.truncated = true;
                continue;
            }
            let segments = validated_segments(&path)?;
            let Ok(file) = open_beneath(&self.root, &segments) else {
                stats.skipped_sources += 1;
                stats.truncated = true;
                continue;
            };
            let meta = file.metadata()?;
            let cap = FILE_CAP.min(WORKSPACE_BYTE_CAP - stats.scanned_bytes);
            if !meta.is_file() || meta.len() > cap {
                stats.skipped_sources += 1;
                stats.truncated = true;
                continue;
            }
            // Charge the maximum permitted read when a file grows beyond the
            // bound. In particular, repeated races cannot bypass the budget.
            let bytes = match bounded_bytes(file, meta.len(), cap) {
                Ok(bytes) => bytes,
                Err(_) => {
                    stats.scanned_bytes += cap;
                    stats.skipped_sources += 1;
                    stats.truncated = true;
                    continue;
                }
            };
            stats.scanned_bytes += bytes.len() as u64;
            if bytes.contains(&0) {
                stats.skipped_sources += 1;
                stats.truncated = true;
                continue;
            }
            let Ok(text) = std::str::from_utf8(&bytes) else {
                stats.skipped_sources += 1;
                stats.truncated = true;
                continue;
            };
            let index = match cache.as_mut() {
                Some(cache) => {
                    let digest = hex::encode(Sha256::digest(&bytes));
                    cache.index(path, lang, text, digest)
                }
                None => analyze(path, lang, text),
            };
            stats.indexed_files += 1;
            stats.truncated |= index.truncated;
            visit(index);
        }
        stats.cache = cache.map(|cache| cache.finish(self));
        Ok(stats)
    }

    /// Case-sensitive exact identifier search over the bounded lexical index.
    pub fn find_symbols(&self, query: &str, limit: usize) -> Result<SymbolSearch> {
        result_limit(limit)?;
        if identifier(query) != Some(query) {
            return Err(LocalReadError::Denied(
                "expected an ASCII identifier of at most 128 bytes",
            ));
        }
        let mut matches = Vec::new();
        let mut results_truncated = false;
        let scan = self.scan_workspace_sources(|index| {
            for symbol in index.symbols {
                if symbol.name != query {
                    continue;
                }
                if matches.len() == limit {
                    results_truncated = true;
                    continue;
                }
                matches.push(SymbolMatch {
                    path: index.path.clone(),
                    language: index.language,
                    name: symbol.name,
                    kind: symbol.kind,
                    line: symbol.line,
                });
            }
        })?;
        Ok(SymbolSearch {
            query: query.into(),
            matches,
            scan,
            results_truncated,
            analysis: "lexical_candidates",
        })
    }

    pub fn dependency_map(&self, limit: usize) -> Result<DependencyMap> {
        result_limit(limit)?;
        let mut indexed_paths = BTreeSet::new();
        let mut references = Vec::new();
        let mut results_truncated = false;
        let scan = self.scan_workspace_sources(|index| {
            indexed_paths.insert(index.path.clone());
            for import in index.imports {
                if references.len() == limit {
                    results_truncated = true;
                    continue;
                }
                references.push((index.path.clone(), index.language, import));
            }
        })?;
        let edges = references
            .into_iter()
            .map(|(source, lang, import)| {
                let candidates = if matches!(lang, "JavaScript" | "TypeScript") {
                    relative_candidates(&source, &import.reference, &indexed_paths)
                } else {
                    Vec::new()
                };
                let status = match candidates.len() {
                    0 => "unresolved",
                    1 => "candidate",
                    _ => "ambiguous",
                };
                DependencyEdge {
                    source,
                    reference: import.reference,
                    line: import.line,
                    status,
                    candidates,
                }
            })
            .collect();
        Ok(DependencyMap {
            edges,
            scan,
            results_truncated,
            analysis: "lexical_import_candidates",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Registry;
    use std::fs;
    #[test]
    fn workspace_queries_are_bounded_and_resolve_only_visible_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".gitignore"), "ignored.ts\n").unwrap();
        fs::write(root.join("ignored.ts"), "function findMe() {}").unwrap();
        fs::write(root.join(".env"), "private=fixture").unwrap();
        fs::write(root.join("src/main.ts"),"import {findMe} from './utils';\nimport x from '../ignored';\nimport y from '../../outside';\nimport z from 'external';\nfunction findMe() {}").unwrap();
        fs::write(root.join("src/utils.ts"), "export function findMe() {}").unwrap();
        fs::write(root.join("src/utils.js"), "export function other() {}").unwrap();
        fs::write(root.join("src/large.ts"), vec![b'a'; FILE_CAP as usize + 1]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("ignored.ts"), root.join("src/link.ts")).unwrap();
        let registry = Registry::new(temp.path().join("state"));
        let grant = registry.grant(&root).unwrap();
        let reader = LocalReader::from_registry(&registry, &grant.workspace_id).unwrap();
        let found = reader.find_symbols("findMe", 200).unwrap();
        assert_eq!(found.matches.len(), 2);
        assert_eq!(found.matches[0].path, "src/main.ts");
        assert_eq!(found.matches[0].line, 5);
        assert_eq!(found.matches[1].path, "src/utils.ts");
        assert!(found.scan.truncated);
        assert_eq!(found.scan.indexed_files, 3);
        assert_eq!(found.scan.skipped_sources, 1);
        assert!(reader.find_symbols("findMe", 1).unwrap().results_truncated);
        assert!(reader.find_symbols("../outside", 10).is_err());
        assert!(reader.find_symbols("findMe", 201).is_err());

        let graph = reader.dependency_map(200).unwrap();
        assert_eq!(graph.edges.len(), 4);
        assert_eq!(graph.edges[0].status, "ambiguous");
        assert_eq!(
            graph.edges[0].candidates,
            vec!["src/utils.js", "src/utils.ts"]
        );
        assert!(
            graph.edges[1..]
                .iter()
                .all(|e| e.status == "unresolved" && e.candidates.is_empty())
        );
        assert!(reader.dependency_map(1).unwrap().results_truncated);
        assert!(
            !serde_json::to_string(&graph)
                .unwrap()
                .contains(&root.display().to_string())
        );
    }

    #[test]
    fn relative_candidates_require_valid_indexed_sources() {
        let files = BTreeSet::from([
            "a.ts".to_owned(),
            "lib/index.ts".to_owned(),
            "data.rs".to_owned(),
            ".env.ts".to_owned(),
        ]);
        assert_eq!(
            relative_candidates("src/main.ts", "../a.ts", &files),
            vec!["a.ts"]
        );
        assert_eq!(
            relative_candidates("main.ts", "./lib", &files),
            vec!["lib/index.ts"]
        );
        assert!(relative_candidates("main.ts", "../a.ts", &files).is_empty());
        assert!(relative_candidates("main.ts", "./data.rs", &files).is_empty());
        assert!(relative_candidates("main.ts", "./.env.ts", &files).is_empty());
        assert!(relative_candidates("main.ts", "a.ts", &files).is_empty());
    }

    #[test]
    fn extracts_bounded_declarations_and_import_references() {
        let cases = [
            (
                "Rust",
                "use crate::store::{User};\npub async fn login() {}\nstruct Session {}",
                "login",
                "crate::store",
            ),
            (
                "Go",
                "import (\n \"example.test/pkg\"\n)\nfunc (s *Store) Login() {}",
                "Login",
                "example.test/pkg",
            ),
            (
                "Python",
                "from app.models import User\nasync def login():\n pass",
                "login",
                "app.models",
            ),
            (
                "TypeScript",
                "import {User} from './models';\nexport async function login() {}",
                "login",
                "./models",
            ),
        ];
        for (lang, text, name, reference) in cases {
            let result = analyze("fixture".into(), lang, text);
            assert_eq!(result.symbols[0].name, name);
            assert_eq!(result.imports[0].reference, reference);
        }
        let result = analyze("x.ts".into(), "TypeScript", &"const item=1;\n".repeat(201));
        assert_eq!(result.symbols.len(), 200);
        assert!(result.truncated);
        assert!(
            import_ref(
                "import x from 'https://example.test/x'",
                "TypeScript",
                false
            )
            .is_none()
        );
        assert!(
            analyze(
                "x.rs".into(),
                "Rust",
                "/*\nfn ignored() {}\n*/\nfn visible() {}"
            )
            .symbols
            .iter()
            .all(|s| s.name == "visible")
        );
    }
    #[test]
    fn source_index_reuses_grant_and_file_limits() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("main.rs"), "fn example() { let private = 123; }").unwrap();
        fs::write(root.join(".env"), "fixture").unwrap();
        fs::write(root.join("large.rs"), vec![b'a'; FILE_CAP as usize + 1]).unwrap();
        let registry = Registry::new(temp.path().join("state"));
        let grant = registry.grant(&root).unwrap();
        let reader = LocalReader::from_registry(&registry, &grant.workspace_id).unwrap();
        let value = serde_json::to_string(&reader.source_index("main.rs").unwrap()).unwrap();
        assert!(value.contains("example"));
        assert!(!value.contains("private"));
        assert!(reader.source_index(".env").is_err());
        assert!(reader.source_index("../outside.rs").is_err());
        assert!(reader.source_index("large.rs").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("main.rs"), root.join("linked.rs")).unwrap();
            assert!(reader.source_index("linked.rs").is_err());
        }
    }
}
