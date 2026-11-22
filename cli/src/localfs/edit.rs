//! Local-only exact-edit with mandatory SHA-256 precondition.
//! No network endpoint or unconstrained write/create is exposed.
//! On Unix all directory traversal and atomic promotion are relative to
//! directory file descriptors (openat + renameat), avoiding symlink escapes.
use super::{
    LocalReadError, LocalReader, MAX_READ_BYTES, Result, bounded_bytes, validated_segments,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    path: String,
    replacements: usize,
    before_hash: String,
    after_hash: String,
    bytes_written: usize,
    atomic: bool,
}

impl LocalReader {
    /// Edit an existing UTF-8 file in an explicitly granted beta workspace.
    /// A matching expected SHA-256 is mandatory; without one, refuse to mutate.
    pub fn edit_exact(
        &self,
        relative: &str,
        expected_sha256: &str,
        old_text: &str,
        new_text: &str,
        replace_all: bool,
    ) -> Result<EditResult> {
        if expected_sha256.len() != 64
            || !expected_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(LocalReadError::Denied(
                "a valid 64-hex SHA-256 precondition is required",
            ));
        }
        if old_text.is_empty()
            || old_text.len() > MAX_READ_BYTES as usize
            || new_text.len() > MAX_READ_BYTES as usize
        {
            return Err(LocalReadError::Denied(
                "old/new edit text exceeds limits or is empty",
            ));
        }
        let segments = validated_segments(relative)?;
        #[cfg(unix)]
        {
            self.edit_unix(&segments, expected_sha256, old_text, new_text, replace_all)
        }
        #[cfg(not(unix))]
        {
            let _ = (segments, old_text, new_text, replace_all);
            Err(LocalReadError::Denied(
                "safe local edits on Windows require a handle-based implementation",
            ))
        }
    }

    #[cfg(unix)]
    fn edit_unix(
        &self,
        segments: &[String],
        expected_sha256: &str,
        old_text: &str,
        new_text: &str,
        replace_all: bool,
    ) -> Result<EditResult> {
        use rustix::fs::{CWD, Mode, OFlags, openat, renameat};
        use std::{
            fs::File,
            io::Write,
            os::unix::fs::{MetadataExt, PermissionsExt},
        };

        let dir_flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::DIRECTORY;
        let mut parent =
            openat(CWD, &self.root, dir_flags, Mode::empty()).map_err(std::io::Error::from)?;
        for component in &segments[..segments.len() - 1] {
            parent = openat(&parent, component.as_str(), dir_flags, Mode::empty())
                .map_err(std::io::Error::from)?;
        }

        let basename = segments
            .last()
            .ok_or(LocalReadError::Denied("invalid file path"))?;
        let read_flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
        let original = File::from(
            openat(&parent, basename.as_str(), read_flags, Mode::empty())
                .map_err(std::io::Error::from)?,
        );
        let metadata = original.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_READ_BYTES {
            return Err(LocalReadError::Denied(
                "editing non-regular or oversized files is forbidden",
            ));
        }
        let current = bounded_bytes(original, metadata.len(), MAX_READ_BYTES)?;
        if current.iter().take(8192).any(|byte| *byte == 0) {
            return Err(LocalReadError::Denied(
                "binary files cannot be exact-edited",
            ));
        }
        let source = std::str::from_utf8(&current)
            .map_err(|_| LocalReadError::Denied("invalid UTF-8 file cannot be edited"))?;
        let before_hash = hex::encode(Sha256::digest(&current));
        if before_hash != expected_sha256.to_ascii_lowercase() {
            return Err(LocalReadError::Denied(
                "SHA-256 precondition failed; file changed since read",
            ));
        }
        let (old_text, new_text) = adapt_line_endings(source, old_text, new_text);
        let count = source.matches(&old_text).count();
        if count == 0 {
            return Err(LocalReadError::Denied("old text not found in current file"));
        }
        if count > 1 && !replace_all {
            return Err(LocalReadError::Denied(
                "old text is ambiguous; require replace_all",
            ));
        }
        let result = if replace_all {
            source.replace(&old_text, &new_text)
        } else {
            source.replacen(&old_text, &new_text, 1)
        };
        if result.len() > MAX_READ_BYTES as usize {
            return Err(LocalReadError::Denied("edited file exceeds 2 MiB limit"));
        }
        let after_hash = hex::encode(Sha256::digest(result.as_bytes()));

        // Create a random unique same-directory temp file through the verified
        // directory handle. TempGuard deletes it on any write/check/rename error.
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|_| LocalReadError::Denied("secure random unavailable"))?;
        let temp_name = format!(".codelocal-beta-{}.tmp", hex::encode(random));
        let mut guard = TempGuard {
            parent: &parent,
            file: temp_name,
            promoted: false,
        };
        let write_flags =
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW;
        let mut temp = File::from(
            openat(
                &parent,
                guard.file.as_str(),
                write_flags,
                Mode::from_bits_truncate(0o600),
            )
            .map_err(std::io::Error::from)?,
        );
        temp.set_permissions(std::fs::Permissions::from_mode(
            metadata.permissions().mode() & 0o777,
        ))?;
        temp.write_all(result.as_bytes())?;
        temp.sync_all()?;

        // Re-open the same directory entry immediately before replacement and
        // verify both content and inode identity. This detects most concurrent
        // edits; non-cooperating external writers can still race this final
        // check and must be handled at the runtime/approval layer.
        let recent = File::from(
            openat(&parent, basename.as_str(), read_flags, Mode::empty())
                .map_err(std::io::Error::from)?,
        );
        let latest_metadata = recent.metadata()?;
        if !latest_metadata.is_file()
            || latest_metadata.ino() != metadata.ino()
            || latest_metadata.dev() != metadata.dev()
            || latest_metadata.len() > MAX_READ_BYTES
        {
            return Err(LocalReadError::Denied("file was replaced during edit"));
        }
        let latest = bounded_bytes(recent, latest_metadata.len(), MAX_READ_BYTES)?;
        if !bool::from(subtle::ConstantTimeEq::ct_eq(
            Sha256::digest(&latest).as_slice(),
            Sha256::digest(&current).as_slice(),
        )) {
            return Err(LocalReadError::Denied("file changed during edit"));
        }
        renameat(&parent, guard.file.as_str(), &parent, basename.as_str())
            .map_err(std::io::Error::from)?;
        guard.promoted = true;
        rustix::fs::fsync(&parent).map_err(std::io::Error::from)?;
        Ok(EditResult {
            path: segments.join("/"),
            replacements: if replace_all { count } else { 1 },
            before_hash,
            after_hash,
            bytes_written: result.len(),
            atomic: true,
        })
    }
}

fn adapt_line_endings(contents: &str, old: &str, new: &str) -> (String, String) {
    if contents.contains(old) {
        return (old.to_owned(), new.to_owned());
    }
    let old_lf = old.replace("\r\n", "\n");
    let new_lf = new.replace("\r\n", "\n");
    if contents.contains("\r\n") && old_lf.contains('\n') {
        let old_crlf = old_lf.replace('\n', "\r\n");
        if contents.contains(&old_crlf) {
            return (old_crlf, new_lf.replace('\n', "\r\n"));
        }
    }
    if !contents.contains("\r\n") && old.contains("\r\n") && contents.contains(&old_lf) {
        return (old_lf, new_lf);
    }
    (old.to_owned(), new.to_owned())
}

#[cfg(unix)]
struct TempGuard<'a> {
    parent: &'a rustix::fd::OwnedFd,
    file: String,
    promoted: bool,
}

#[cfg(unix)]
impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        if !self.promoted {
            let _ = rustix::fs::unlinkat(
                self.parent,
                self.file.as_str(),
                rustix::fs::AtFlags::empty(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Registry;
    use std::fs;

    fn setup() -> (tempfile::TempDir, LocalReader) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("main.rs"), b"let value = 1;\nlet value = 1;\n").unwrap();
        let registry = Registry::new(temp.path().join("registry"));
        let grant = registry.grant(project).unwrap();
        let reader = LocalReader::from_registry(&registry, &grant.workspace_id).unwrap();
        (temp, reader)
    }

    fn hash(data: &[u8]) -> String {
        hex::encode(Sha256::digest(data))
    }

    #[cfg(unix)]
    #[test]
    fn replaces_exact_text_with_hash_and_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (temp, reader) = setup();
        let path = temp.path().join("project/main.rs");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let current = fs::read(&path).unwrap();
        let result = reader
            .edit_exact("main.rs", &hash(&current), "value = 1", "value = 9", true)
            .unwrap();
        assert_eq!(result.replacements, 2);
        assert_eq!(result.before_hash, hash(&current));
        let updated = fs::read(&path).unwrap();
        assert_eq!(updated, b"let value = 9;\nlet value = 9;\n");
        assert_eq!(result.after_hash, hash(&updated));
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[cfg(unix)]
    #[test]
    fn mismatch_and_ambiguous_text_do_not_modify_file() {
        let (temp, reader) = setup();
        let path = temp.path().join("project/main.rs");
        let original = fs::read(&path).unwrap();
        assert!(
            reader
                .edit_exact("main.rs", &"0".repeat(64), "value = 1", "value = 2", true)
                .is_err()
        );
        assert!(
            reader
                .edit_exact("main.rs", &hash(&original), "value = 1", "value = 2", false)
                .is_err()
        );
        assert!(reader.edit_exact("main.rs", "", "a", "b", false).is_err());
        assert!(
            reader
                .edit_exact("main.rs", &hash(&original), "", "b", false)
                .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_sensitive_files_and_symlink_escape() {
        use std::os::unix::fs::symlink;
        let (temp, reader) = setup();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside.rs");
        fs::write(&outside, b"let password = 'secret'").unwrap();
        symlink(&outside, root.join("link.rs")).unwrap();
        fs::write(root.join(".env"), b"API_KEY=real").unwrap();
        let precondition = hash(b"let password = 'secret'");
        assert!(
            reader
                .edit_exact("link.rs", &precondition, "secret", "spoofed", true)
                .is_err()
        );
        assert!(
            reader
                .edit_exact("../outside.rs", &precondition, "secret", "spoofed", true)
                .is_err()
        );
        assert!(
            reader
                .edit_exact(".env", &hash(b"API_KEY=real"), "real", "spoofed", true)
                .is_err()
        );
        assert_eq!(fs::read(outside).unwrap(), b"let password = 'secret'");
    }

    #[cfg(unix)]
    #[test]
    fn edits_preserving_crlf_when_go_style_lf_old_text_supplied() {
        let (temp, reader) = setup();
        let path = temp.path().join("project/crlf.rs");
        let data = b"first line\r\nsecond line\r\n";
        fs::write(&path, data).unwrap();
        reader
            .edit_exact(
                "crlf.rs",
                &hash(data),
                "first line\nsecond line",
                "one line\ntwo lines",
                false,
            )
            .unwrap();
        assert_eq!(fs::read(path).unwrap(), b"one line\r\ntwo lines\r\n");
    }

    #[cfg(unix)]
    #[test]
    fn cleans_temporary_files_and_restricts_oversized_edits() {
        let (temp, reader) = setup();
        let root = temp.path().join("project");
        let path = root.join("main.rs");
        let old = fs::read(&path).unwrap();
        assert!(
            reader
                .edit_exact("main.rs", &hash(&old), "not here", "new", true)
                .is_err()
        );
        assert!(
            reader
                .edit_exact(
                    "main.rs",
                    &hash(&old),
                    "value = 1",
                    &"x".repeat(MAX_READ_BYTES as usize),
                    true
                )
                .is_err()
        );
        assert!(!fs::read_dir(root).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".codelocal-beta-")
        }));
    }
}
