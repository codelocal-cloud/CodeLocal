//! Bounded lexical declaration/import inventory. Not a semantic resolver.
use super::{LocalReadError, LocalReader, Result, bounded_bytes, open_beneath, validated_segments};
use serde::Serialize;
use std::path::Path;

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

#[derive(Debug, Serialize)]
struct Symbol {
    name: String,
    kind: &'static str,
    line: usize,
}

#[derive(Debug, Serialize)]
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
                    kind,
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

impl LocalReader {
    /// Index a single explicitly requested file without returning source text.
    pub fn source_index(&self, path: &str) -> Result<SourceIndex> {
        let segments = validated_segments(path)?;
        let relative = segments.join("/");
        let lang = match Path::new(&relative).extension().and_then(|v| v.to_str()) {
            Some("rs") => "Rust",
            Some("go") => "Go",
            Some("py" | "pyi") => "Python",
            Some("ts" | "tsx" | "mts" | "cts") => "TypeScript",
            Some("js" | "jsx" | "mjs" | "cjs") => "JavaScript",
            _ => {
                return Err(LocalReadError::Denied(
                    "source index supports Rust, Go, Python, TypeScript and JavaScript",
                ));
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Registry;
    use std::fs;
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
