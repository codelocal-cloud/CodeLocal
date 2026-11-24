//! Opt-in, bounded persistent cache of lexical candidates, never raw source.
//! Rewalk and hash visible files before using any cached entry. Cache errors
//! never relax the grant, ignore rules or file budgets.
use super::{Import, LocalReader, SourceIndex, Symbol, analyze, identifier, module_reference};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

const CACHE_FILE: &str = "index-cache-v1.json";
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;
const VERSION: u32 = 1;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CacheStats {
    hits: usize,
    misses: usize,
    persisted: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    sha256: String,
    symbols: Vec<Symbol>,
    imports: Vec<Import>,
    truncated: bool,
}

impl Entry {
    fn valid(&self, digest: &str, lines: usize) -> bool {
        self.sha256 == digest
            && self.symbols.len() <= super::ITEM_CAP
            && self.imports.len() <= super::ITEM_CAP
            && self.symbols.iter().all(|s| {
                identifier(&s.name) == Some(s.name.as_str())
                    && (1..=lines).contains(&s.line)
                    && matches!(
                        s.kind.as_str(),
                        "function"
                            | "struct"
                            | "enum"
                            | "trait"
                            | "type"
                            | "module"
                            | "constant"
                            | "static"
                            | "method"
                            | "variable"
                            | "class"
                            | "interface"
                    )
            })
            && self.imports.iter().all(|i| {
                module_reference(&i.reference) == Some(i.reference.as_str())
                    && (1..=lines).contains(&i.line)
            })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    workspace: String,
    root_hash: String,
    grant_at: u64,
    entries: BTreeMap<String, Entry>,
}

pub(super) struct ScanCache {
    prior: BTreeMap<String, Entry>,
    next: Snapshot,
    stats: CacheStats,
    entry_bytes: usize,
}

impl ScanCache {
    pub(super) fn start(reader: &LocalReader) -> Option<Self> {
        if !reader.index_cache_enabled {
            return None;
        }
        let dir = private_state_dir(&reader.index_state_dir, &reader.root)?;
        let root_hash = hex::encode(Sha256::digest(reader.root.to_str()?.as_bytes()));
        let next = Snapshot {
            version: VERSION,
            workspace: reader.workspace_id.clone(),
            root_hash,
            grant_at: reader.grant_at,
            entries: BTreeMap::new(),
        };
        let prior = read_snapshot(&dir)
            .filter(|old| {
                old.version == VERSION
                    && old.workspace == next.workspace
                    && old.root_hash == next.root_hash
                    && old.grant_at == next.grant_at
                    && old.entries.len() <= super::WORKSPACE_FILE_CAP
            })
            .map(|old| old.entries)
            .unwrap_or_default();
        Some(Self {
            prior,
            next,
            stats: CacheStats::default(),
            entry_bytes: 0,
        })
    }

    pub(super) fn index(
        &mut self,
        path: String,
        lang: &'static str,
        text: &str,
        digest: String,
    ) -> SourceIndex {
        let cached = self
            .prior
            .remove(&path)
            .filter(|entry| entry.valid(&digest, text.lines().count()));
        let index = if let Some(entry) = cached {
            self.stats.hits += 1;
            SourceIndex {
                path,
                language: lang,
                symbols: entry.symbols,
                imports: entry.imports,
                truncated: entry.truncated,
                analysis: "lexical_candidates",
            }
        } else {
            self.stats.misses += 1;
            analyze(path, lang, text)
        };
        let entry = Entry {
            sha256: digest,
            symbols: index.symbols.clone(),
            imports: index.imports.clone(),
            truncated: index.truncated,
        };
        // Bound the accumulated serialized payload as well as the final file.
        if let Ok(bytes) = serde_json::to_vec(&(&index.path, &entry)) {
            let cost = bytes.len().saturating_add(8);
            if self.entry_bytes.saturating_add(cost) <= MAX_CACHE_BYTES - 4096 {
                self.entry_bytes += cost;
                self.next.entries.insert(index.path.clone(), entry);
            }
        }
        index
    }

    pub(super) fn finish(mut self, reader: &LocalReader) -> CacheStats {
        // Do not persist a scan after the grant has been revoked or replaced.
        let current = crate::workspace::Registry::new(&reader.index_state_dir)
            .list()
            .ok()
            .is_some_and(|grants| {
                grants.iter().any(|grant| {
                    grant.workspace_id == reader.workspace_id
                        && grant.granted_at == reader.grant_at
                        && grant.local_path == reader.root
                        && !grant.managed
                        && !grant.system
                        && !grant.system_app
                        && !grant.hidden
                })
            });
        if current && let Some(dir) = private_state_dir(&reader.index_state_dir, &reader.root) {
            self.stats.persisted = write_snapshot(&dir, &self.next);
        }
        self.stats
    }
}

// Initially Unix-only; Windows ACL and reparse-point parity is not claimed.
#[cfg(unix)]
fn private_state_dir(state: &Path, root: &Path) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::symlink_metadata(state).ok()?;
    if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return None;
    }
    let dir = std::fs::canonicalize(state).ok()?;
    if dir.starts_with(root) {
        return None;
    }
    Some(dir)
}

#[cfg(not(unix))]
fn private_state_dir(_state: &Path, _root: &Path) -> Option<std::path::PathBuf> {
    None
}

#[cfg(unix)]
fn read_snapshot(dir: &Path) -> Option<Snapshot> {
    use std::os::unix::fs::MetadataExt;
    let file = super::open_beneath(dir, &[CACHE_FILE.to_owned()]).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file()
        || meta.mode() & 0o077 != 0
        || meta.nlink() != 1
        || meta.uid() != std::fs::metadata(dir).ok()?.uid()
    {
        return None;
    }
    let bytes = super::bounded_bytes(file, meta.len(), MAX_CACHE_BYTES as u64).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(not(unix))]
fn read_snapshot(_dir: &Path) -> Option<Snapshot> {
    None
}

#[cfg(unix)]
fn write_snapshot(dir: &Path, snapshot: &Snapshot) -> bool {
    use std::{io::Write, os::unix::fs::PermissionsExt};
    let path = dir.join(CACHE_FILE);
    // Never replace a symlink, directory or other special file.
    if std::fs::symlink_metadata(&path).is_ok_and(|meta| !meta.is_file()) {
        return false;
    }
    let Ok(bytes) = serde_json::to_vec(snapshot) else {
        return false;
    };
    if bytes.len() > MAX_CACHE_BYTES {
        return false;
    }
    let Ok(mut tmp) = tempfile::NamedTempFile::new_in(dir) else {
        return false;
    };
    if tmp
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .is_err()
        || tmp.write_all(&bytes).is_err()
        || tmp.as_file().sync_all().is_err()
        || tmp.persist(path).is_err()
    {
        return false;
    }
    std::fs::File::open(dir)
        .and_then(|dir| dir.sync_all())
        .is_ok()
}

#[cfg(not(unix))]
fn write_snapshot(_dir: &Path, _snapshot: &Snapshot) -> bool {
    false
}

// User-initiated removal of the fixed cache file, never a recursive deletion.
pub(crate) fn clear(state: &Path) -> std::io::Result<bool> {
    let path = state.join(CACHE_FILE);
    match std::fs::symlink_metadata(state) {
        Ok(meta) if !meta.is_dir() => {
            return Err(std::io::Error::other("invalid cache state directory"));
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::workspace::Registry;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn cache_is_opt_in_revalidated_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("main.rs"), "fn old() {}").unwrap();
        fs::write(root.join("keep.rs"), "fn keep() {} // PRIVATE_SOURCE_BODY").unwrap();
        let registry = Registry::new(temp.path().join("state"));
        let grant = registry.grant(&root).unwrap();
        let reader = || {
            let mut reader = LocalReader::from_registry(&registry, &grant.workspace_id).unwrap();
            reader.index_cache_enabled = true;
            reader
        };
        let mut disabled = reader();
        disabled.index_cache_enabled = false;
        assert!(
            disabled
                .find_symbols("old", 5)
                .unwrap()
                .scan
                .cache
                .is_none()
        );
        let path = registry.state_dir().join(CACHE_FILE);
        assert!(!path.exists());

        let cold = reader().find_symbols("old", 5).unwrap();
        assert_eq!(cold.matches.len(), 1);
        let stats = cold.scan.cache.unwrap();
        assert_eq!((stats.hits, stats.misses, stats.persisted), (0, 2, true));
        let stored = fs::read_to_string(&path).unwrap();
        assert!(!stored.contains("PRIVATE_SOURCE_BODY"));
        assert!(!stored.contains(root.to_str().unwrap()));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let warm = reader().find_symbols("old", 5).unwrap();
        assert_eq!(warm.matches.len(), 1);
        let stats = warm.scan.cache.unwrap();
        assert_eq!((stats.hits, stats.misses), (2, 0));

        // Same-size replacement must invalidate regardless of timestamp precision.
        fs::write(root.join("main.rs"), "fn new() {}").unwrap();
        let changed = reader().find_symbols("new", 5).unwrap();
        assert_eq!(changed.matches.len(), 1);
        let stats = changed.scan.cache.unwrap();
        assert_eq!((stats.hits, stats.misses), (1, 1));
        assert!(reader().find_symbols("old", 5).unwrap().matches.is_empty());

        // Re-evaluate gitignore and remove cached paths even when unchanged.
        fs::write(root.join(".gitignore"), "main.rs\n").unwrap();
        let ignored = reader().find_symbols("new", 5).unwrap();
        assert!(ignored.matches.is_empty());
        assert_eq!(ignored.scan.cache.unwrap().hits, 1);
        assert!(!fs::read_to_string(&path).unwrap().contains("main.rs"));
        fs::remove_file(root.join("keep.rs")).unwrap();
        let deleted = reader().find_symbols("keep", 5).unwrap();
        assert!(deleted.matches.is_empty());
        assert!(!fs::read_to_string(&path).unwrap().contains("keep.rs"));

        // Cache corruption and oversized files are treated as misses.
        fs::write(root.join("fresh.rs"), "fn fresh() {}").unwrap();
        fs::write(&path, "{invalid").unwrap();
        assert_eq!(
            reader()
                .find_symbols("fresh", 5)
                .unwrap()
                .scan
                .cache
                .unwrap()
                .misses,
            1
        );
        fs::write(&path, vec![b' '; MAX_CACHE_BYTES + 1]).unwrap();
        assert_eq!(
            reader()
                .find_symbols("fresh", 5)
                .unwrap()
                .scan
                .cache
                .unwrap()
                .misses,
            1
        );
        assert!(fs::metadata(&path).unwrap().len() <= MAX_CACHE_BYTES as u64);
        assert!(clear(registry.state_dir()).unwrap());
        assert!(!clear(registry.state_dir()).unwrap());
    }

    #[test]
    fn cache_does_not_cross_workspaces_or_follow_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a");
        let b = temp.path().join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        for root in [&a, &b] {
            fs::write(root.join("main.rs"), "fn same() {}").unwrap();
        }
        let registry = Registry::new(temp.path().join("state"));
        let ga = registry.grant(&a).unwrap();
        let gb = registry.grant(&b).unwrap();
        let reader = |id: &str| {
            let mut r = LocalReader::from_registry(&registry, id).unwrap();
            r.index_cache_enabled = true;
            r
        };
        assert_eq!(
            reader(&ga.workspace_id)
                .find_symbols("same", 5)
                .unwrap()
                .scan
                .cache
                .unwrap()
                .misses,
            1
        );
        assert_eq!(
            reader(&gb.workspace_id)
                .find_symbols("same", 5)
                .unwrap()
                .scan
                .cache
                .unwrap()
                .misses,
            1
        );
        let path = registry.state_dir().join(CACHE_FILE);
        fs::remove_file(&path).unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, "do not touch").unwrap();
        symlink(&outside, &path).unwrap();
        let stats = reader(&gb.workspace_id)
            .find_symbols("same", 5)
            .unwrap()
            .scan
            .cache
            .unwrap();
        assert_eq!((stats.hits, stats.misses, stats.persisted), (0, 1, false));
        assert_eq!(fs::read_to_string(&outside).unwrap(), "do not touch");
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(registry.revoke(&gb.workspace_id).unwrap());
        assert!(LocalReader::from_registry(&registry, &gb.workspace_id).is_err());
    }
}
