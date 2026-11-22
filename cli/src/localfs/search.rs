//! Local-only, Git-ignore-aware search. File content is opened through the
//! same openat/O_NOFOLLOW boundary as the beta local reader.
use super::{
    LocalReadError, LocalReader, MAX_READ_BYTES, Result, bounded_bytes, open_beneath,
    validated_segments,
};
use ignore::{DirEntry, WalkBuilder};
use serde::Serialize;
use std::{path::Path, str};

const MAX_WALK_ENTRIES: usize = 10_000;
const MAX_WALK_DEPTH: usize = 24;
const MAX_RESULTS: usize = 200;
const MAX_TEXT_SCAN_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearch {
    matches: Vec<String>,
    truncated: bool,
    scanned_entries: usize,
    include_ignored: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSearch {
    matches: Vec<String>,
    truncated: bool,
    scanned_files: usize,
    scanned_bytes: u64,
    include_ignored: bool,
}

impl LocalReader {
    /// Search names of files below a beta-granted workspace. Uses .gitignore,
    /// ignores sensitive paths and never follows symlinks.
    pub fn search_files(&self, query: &str, max_results: usize) -> Result<FileSearch> {
        valid_query(query)?;
        let limit = valid_limit(max_results)?;
        let mut matches = Vec::new();
        let mut scanned = 0usize;
        let mut truncated = false;
        for entry in self.walk() {
            let entry =
                entry.map_err(|_| LocalReadError::Denied("workspace search traversal failed"))?;
            if entry.depth() == 0 {
                continue;
            }
            scanned += 1;
            if scanned > MAX_WALK_ENTRIES {
                truncated = true;
                break;
            }
            if entry.file_type().is_some_and(|kind| kind.is_file()) {
                let relative = self.relative_name(&entry)?;
                if relative.contains(query) {
                    if matches.len() == limit {
                        truncated = true;
                        break;
                    }
                    matches.push(relative);
                }
            }
        }
        Ok(FileSearch {
            matches,
            truncated,
            scanned_entries: scanned.min(MAX_WALK_ENTRIES),
            include_ignored: false,
        })
    }

    /// Fixed-string UTF-8 search (not regex) returning Go-compatible
    /// "path:line:column:text" matches. No subprocess or shell involvement.
    pub fn search_text(&self, query: &str, max_results: usize) -> Result<TextSearch> {
        valid_query(query)?;
        let limit = valid_limit(max_results)?;
        let mut matches = Vec::new();
        let mut scanned_files = 0usize;
        let mut scanned_entries = 0usize;
        let mut scanned_bytes = 0u64;
        let mut truncated = false;
        'walk: for entry in self.walk() {
            let entry =
                entry.map_err(|_| LocalReadError::Denied("workspace search traversal failed"))?;
            if entry.depth() == 0 {
                continue;
            }
            scanned_entries += 1;
            if scanned_entries > MAX_WALK_ENTRIES {
                truncated = true;
                break;
            }
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let relative = self.relative_name(&entry)?;
            let segments = validated_segments(&relative)?;
            let file = match open_beneath(&self.root, &segments) {
                Ok(file) => file,
                // A symlink/raced path can never be searched, even if the walker
                // observed it earlier as a regular file.
                Err(_) => continue,
            };
            let meta = file.metadata()?;
            if !meta.is_file() || meta.len() > MAX_READ_BYTES {
                continue;
            }
            if scanned_bytes.saturating_add(meta.len()) > MAX_TEXT_SCAN_BYTES {
                truncated = true;
                break;
            }
            let bytes = bounded_bytes(file, meta.len(), MAX_READ_BYTES)?;
            scanned_files += 1;
            scanned_bytes += bytes.len() as u64;
            if bytes.iter().take(8192).any(|b| *b == 0) {
                continue;
            }
            let Ok(contents) = str::from_utf8(&bytes) else {
                continue;
            };
            for (line_index, line) in contents.lines().enumerate() {
                // No per-line allocations except for the selected matches.
                for (offset, _) in line.match_indices(query) {
                    if matches.len() == limit {
                        truncated = true;
                        break 'walk;
                    }
                    // Avoid exposing arbitrarily long lines of unrelated source.
                    let trimmed = line.trim_end_matches('\r');
                    let sample: String = trimmed.chars().take(1024).collect();
                    matches.push(format!(
                        "{relative}:{}:{}:{sample}",
                        line_index + 1,
                        offset + 1,
                    ));
                }
            }
        }
        Ok(TextSearch {
            matches,
            truncated,
            scanned_files,
            scanned_bytes,
            include_ignored: false,
        })
    }

    fn relative_name(&self, entry: &DirEntry) -> Result<String> {
        let relative = entry
            .path()
            .strip_prefix(&self.root)
            .map_err(|_| LocalReadError::Denied("workspace path escaped grant"))?;
        let value = relative.to_str().ok_or(LocalReadError::InvalidUtf8)?;
        let segments = validated_segments(value)?;
        Ok(segments.join("/"))
    }

    fn walk(&self) -> impl Iterator<Item = std::result::Result<DirEntry, ignore::Error>> + '_ {
        let root = self.root.clone();
        let mut builder = WalkBuilder::new(&root);
        builder
            .max_depth(Some(MAX_WALK_DEPTH))
            .hidden(false) // portable .codelocal rules remain visible
            .follow_links(false)
            // Deterministic order across APFS/ext4, including truncated pages.
            .sort_by_file_name(|a, b| a.cmp(b))
            .git_ignore(true)
            .git_exclude(true)
            .git_global(true)
            .ignore(true)
            .require_git(false)
            .threads(1)
            .filter_entry(move |entry| safe_walk_entry(&root, entry));
        builder.build()
    }
}

fn valid_query(query: &str) -> Result<()> {
    if query.is_empty() || query.len() > 256 || query.chars().any(char::is_control) {
        return Err(LocalReadError::Denied(
            "search query must be 1-256 printable characters",
        ));
    }
    Ok(())
}

fn valid_limit(limit: usize) -> Result<usize> {
    if !(1..=MAX_RESULTS).contains(&limit) {
        return Err(LocalReadError::Denied(
            "search maxResults must be between 1 and 200",
        ));
    }
    Ok(limit)
}

fn safe_walk_entry(root: &Path, entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    if entry.file_type().is_none_or(|kind| kind.is_symlink()) {
        return false;
    }
    let Ok(relative) = entry.path().strip_prefix(root) else {
        return false;
    };
    let Some(relative) = relative.to_str() else {
        return false;
    };
    // Portable CodeLocal rules are indexable, but hidden runtime dirs never are.
    if entry.file_type().is_some_and(|kind| kind.is_dir())
        && matches!(
            relative,
            ".codelocal" | ".codelocal/rules" | ".codelocal/skills"
        )
    {
        return true;
    }
    validated_segments(relative).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Registry;
    use std::fs;

    fn setup() -> (tempfile::TempDir, LocalReader) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(project.join(".gitignore"), "ignored.txt\nbuild/\n").unwrap();
        fs::write(project.join("src/main.rs"), "hello world\nhello again\n").unwrap();
        fs::write(project.join("src/lib.rs"), "hello library\n").unwrap();
        fs::write(project.join("ignored.txt"), "hello SECRET_IGNORE\n").unwrap();
        fs::write(project.join(".env"), "hello SECRET_ENV\n").unwrap();
        fs::write(project.join("build.txt"), "hello filename\n").unwrap();
        fs::create_dir(project.join("build")).unwrap();
        fs::write(project.join("build/secret.rs"), "hello SECRET_BUILD\n").unwrap();
        let registry = Registry::new(temp.path().join("state"));
        let grant = registry.grant(project).unwrap();
        let reader = LocalReader::from_registry(&registry, &grant.workspace_id).unwrap();
        (temp, reader)
    }

    #[test]
    fn searches_files_with_gitignore_and_sensitive_exclusions() {
        let (_temp, reader) = setup();
        let files = serde_json::to_value(reader.search_files(".rs", 20).unwrap()).unwrap();
        assert_eq!(
            files["matches"],
            serde_json::json!(["src/lib.rs", "src/main.rs"])
        );
        assert_eq!(files["includeIgnored"], false);
        assert_eq!(files["truncated"], false);
        assert!(reader.search_files("../", 20).unwrap().matches.is_empty());
        assert!(reader.search_files("", 20).is_err());
        assert!(reader.search_files("a", 0).is_err());
        assert!(reader.search_files("a", 201).is_err());
    }

    #[test]
    fn fixed_text_search_has_go_style_line_and_column() {
        let (_temp, reader) = setup();
        let result = reader.search_text("hello", 20).unwrap();
        assert!(
            result
                .matches
                .contains(&"src/main.rs:1:1:hello world".into())
        );
        assert!(
            result
                .matches
                .contains(&"src/main.rs:2:1:hello again".into())
        );
        assert!(
            result
                .matches
                .contains(&"src/lib.rs:1:1:hello library".into())
        );
        assert!(!result.matches.iter().any(|value| value.contains("SECRET_")));
        assert!(!result.include_ignored);
    }

    #[test]
    fn respects_result_limit_and_does_not_search_symlinks() {
        let (temp, reader) = setup();
        let result = reader.search_text("hello", 1).unwrap();
        assert_eq!(result.matches.len(), 1);
        assert!(result.truncated);
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(
                temp.path().join("project/src"),
                temp.path().join("project/src_link"),
            )
            .unwrap();
            symlink(
                temp.path().join("project/src/main.rs"),
                temp.path().join("project/link.rs"),
            )
            .unwrap();
            let hits = reader.search_files(".rs", 50).unwrap();
            assert!(
                !hits
                    .matches
                    .iter()
                    .any(|name| name.starts_with("src_link") || name == "link.rs")
            );
        }
    }

    #[test]
    fn portable_rules_are_visible_but_secrets_are_not() {
        let (temp, reader) = setup();
        let rules = temp.path().join("project/.codelocal/rules");
        fs::create_dir_all(&rules).unwrap();
        fs::write(rules.join("agent.md"), "hello safe instructions").unwrap();
        fs::write(rules.join("secrets.json"), "hello do not print").unwrap();
        let result = reader.search_text("hello", 100).unwrap();
        assert!(
            result
                .matches
                .iter()
                .any(|m| m.starts_with(".codelocal/rules/agent.md:"))
        );
        assert!(!result.matches.iter().any(|m| m.contains("do not print")));
    }

    #[test]
    fn refuses_oversized_files_and_bounds_file_search() {
        let (temp, reader) = setup();
        fs::write(
            temp.path().join("project/src/huge.txt"),
            vec![b'x'; (MAX_READ_BYTES + 1) as usize],
        )
        .unwrap();
        let result = reader.search_text("hello", 100).unwrap();
        assert!(!result.matches.iter().any(|m| m.contains("huge.txt")));
        assert!(reader.search_text("hello", 0).is_err());
        assert!(reader.search_text("hello", 201).is_err());
    }
}
