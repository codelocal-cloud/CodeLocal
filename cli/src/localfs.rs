//! Beta-only local file inspection engine for the eventual compact MCP "read" tool.
//! There is intentionally no network handler or process execution entrypoint here.
//! All access requires a local workspace grant and is read-only.
mod edit;
mod search;
pub use edit::EditResult;
pub use search::{FileSearch, ProjectMap, TextSearch};

use crate::workspace::{Registry, Workspace, id_for_path, validate_workspace_path};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt,
    fs::{self, File, Metadata},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_READ_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BATCH_BYTES: u64 = 8 * 1024 * 1024;
const MAX_BATCH_FILES: usize = 20;
const MAX_HASH_BYTES: u64 = 4 * MAX_READ_BYTES;

#[derive(Debug)]
pub enum LocalReadError {
    Denied(&'static str),
    Io(io::Error),
    InvalidUtf8,
    NotFound,
}
impl fmt::Display for LocalReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(reason) => write!(f, "read denied: {reason}"),
            Self::Io(err) => write!(f, "local read failed: {err}"),
            Self::InvalidUtf8 => f.write_str("workspace path must contain valid UTF-8"),
            Self::NotFound => f.write_str("workspace grant not found (run codelocal grant first)"),
        }
    }
}
impl Error for LocalReadError {}
impl From<io::Error> for LocalReadError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
type Result<T> = std::result::Result<T, LocalReadError>;

/// Partial Go localfs FileInfo/Read payload, with no absolute local machine paths.
/// Ignored/gitignore semantics are not yet implemented; omit that field rather
/// than falsely claiming an ignored path is visible in a production index.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    path: String,
    size: u64,
    mtime_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignored: Option<bool>,
    is_file: bool,
    is_directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    binary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_lines: Option<usize>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBatch {
    files: Vec<FileData>,
    total_bytes: u64,
}

pub struct LocalReader {
    workspace_id: String,
    root: PathBuf,
}

impl LocalReader {
    /// Only construct an engine with the exact local grant in the beta workspace registry.
    pub fn from_registry(registry: &Registry, workspace_id: &str) -> Result<Self> {
        if workspace_id.is_empty() || workspace_id.len() > 128 {
            return Err(LocalReadError::NotFound);
        }
        let grants = registry
            .list()
            .map_err(|_| LocalReadError::Denied("workspace registry invalid"))?;
        let grant = grants
            .iter()
            .find(|w| w.workspace_id == workspace_id)
            .ok_or(LocalReadError::NotFound)?;
        Self::for_grant(grant)
    }

    fn for_grant(grant: &Workspace) -> Result<Self> {
        if grant.managed || grant.system_app || grant.hidden || grant.system {
            return Err(LocalReadError::Denied(
                "system and managed workspaces require a separate policy",
            ));
        }
        let root = fs::canonicalize(&grant.local_path)?;
        if !root.is_dir() || root != grant.local_path || id_for_path(&root) != grant.workspace_id {
            return Err(LocalReadError::Denied(
                "workspace grant no longer matches filesystem root",
            ));
        }
        validate_workspace_path(&root).map_err(|_| {
            LocalReadError::Denied("credential/config directory or filesystem root cannot be read")
        })?;
        // No Go daemon state is shared; open only files in this beta grant.
        Ok(Self {
            workspace_id: grant.workspace_id.clone(),
            root,
        })
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn info(&self, relative: &str) -> Result<FileData> {
        let segments = validated_segments(relative)?;
        let file = open_beneath(&self.root, &segments)?;
        let meta = file.metadata()?;
        let mut out = base_data(&segments, &meta);
        if meta.is_file() && meta.len() <= MAX_HASH_BYTES {
            let bytes = bounded_bytes(file, meta.len(), MAX_HASH_BYTES)?;
            out.hash = Some(hex::encode(Sha256::digest(bytes)));
        }
        Ok(out)
    }

    pub fn read(
        &self,
        relative: &str,
        start_line: Option<usize>,
        end_line: Option<usize>,
    ) -> Result<FileData> {
        let start = start_line.unwrap_or(1);
        let end = end_line.unwrap_or(0);
        if start == 0 || (end != 0 && end < start) {
            return Err(LocalReadError::Denied("invalid line range"));
        }
        let segments = validated_segments(relative)?;
        let file = open_beneath(&self.root, &segments)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(LocalReadError::Denied("not a regular file"));
        }
        // Conservative initial migration: unlike Go, all range reads are bounded.
        let bytes = bounded_bytes(file, meta.len(), MAX_READ_BYTES)?;
        let mut out = base_data(&segments, &meta);
        out.hash = Some(hex::encode(Sha256::digest(&bytes)));
        if bytes.iter().take(8192).any(|b| *b == 0) {
            out.binary = Some(true);
            return Ok(out);
        }
        let Ok(contents) = std::str::from_utf8(&bytes) else {
            out.binary = Some(true);
            return Ok(out);
        };
        let lines = if contents.is_empty() {
            Vec::new()
        } else {
            contents
                .split_terminator('\n')
                .map(|line| line.trim_end_matches('\r'))
                .collect::<Vec<_>>()
        };
        let total = lines.len();
        let selected = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index >= start - 1 && (end == 0 || *index < end))
            .map(|(_, line)| *line)
            .collect::<Vec<_>>();
        out.binary = Some(false);
        out.content = Some(selected.join("\n"));
        out.start_line = Some(start);
        out.end_line = Some(if end == 0 { total } else { end.min(total) });
        out.total_lines = Some(total);
        Ok(out)
    }

    pub fn many(&self, paths: &[String]) -> Result<FileBatch> {
        if paths.len() > MAX_BATCH_FILES {
            return Err(LocalReadError::Denied("batch exceeds file count limit"));
        }
        // Preflight every item and its size before reading the first file.
        let mut total = 0u64;
        for relative in paths {
            let segments = validated_segments(relative)?;
            let file = open_beneath(&self.root, &segments)?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err(LocalReadError::Denied("batch entry is not a regular file"));
            }
            if meta.len() > MAX_READ_BYTES {
                return Err(LocalReadError::Denied(
                    "batch entry exceeds per-file read limit",
                ));
            }
            total = total
                .checked_add(meta.len())
                .ok_or(LocalReadError::Denied("batch size overflow"))?;
            if total > MAX_BATCH_BYTES {
                return Err(LocalReadError::Denied("batch exceeds 8MiB limit"));
            }
        }
        let files = paths
            .iter()
            .map(|path| self.read(path, None, None))
            .collect::<Result<Vec<_>>>()?;
        Ok(FileBatch {
            files,
            total_bytes: total,
        })
    }
}

fn base_data(segments: &[String], meta: &Metadata) -> FileData {
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as u64);
    FileData {
        path: segments.join("/"),
        size: meta.len(),
        mtime_ms,
        ignored: None,
        is_file: meta.is_file(),
        is_directory: meta.is_dir(),
        hash: None,
        binary: None,
        content: None,
        start_line: None,
        end_line: None,
        total_lines: None,
    }
}

fn bounded_bytes(mut file: File, advertised_size: u64, cap: u64) -> Result<Vec<u8>> {
    if advertised_size > cap {
        return Err(LocalReadError::Denied("file exceeds bounded read limit"));
    }
    let mut bytes = Vec::with_capacity((advertised_size.min(cap)) as usize);
    file.by_ref().take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(LocalReadError::Denied(
            "file expanded beyond bounded read limit",
        ));
    }
    Ok(bytes)
}

fn validated_segments(relative: &str) -> Result<Vec<String>> {
    if relative.is_empty()
        || relative.len() > 4096
        || relative.contains('\0')
        || relative.contains('\\')
        || relative.starts_with('~')
    {
        return Err(LocalReadError::Denied("invalid workspace-relative path"));
    }
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err(LocalReadError::Denied("absolute paths are forbidden"));
    }
    let mut segments = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(name) => {
                let name = name.to_str().ok_or(LocalReadError::InvalidUtf8)?;
                if name.len() > 255 || name.is_empty() || name.chars().any(char::is_control) {
                    return Err(LocalReadError::Denied("invalid path component"));
                }
                segments.push(name.to_owned());
            }
            Component::CurDir => {}
            _ => {
                return Err(LocalReadError::Denied(
                    "parent traversal, prefix and absolute paths forbidden",
                ));
            }
        }
    }
    if segments.is_empty() {
        return Err(LocalReadError::Denied("path to a file is required"));
    }
    if sensitive(&segments) {
        return Err(LocalReadError::Denied(
            "credential, runtime, or sensitive path",
        ));
    }
    Ok(segments)
}

fn sensitive(segments: &[String]) -> bool {
    let components = segments
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let first = components[0].as_str();
    if first == ".codelocal" {
        // CodeLocal portable project context is allowed; private runtime state is not.
        if components.len() == 2
            && matches!(components[1].as_str(), "project.json" | "quality.json")
        {
            return false;
        }
        if components.len() >= 3 && matches!(components[1].as_str(), "rules" | "skills") {
            return components[2..]
                .iter()
                .any(|part| !safe_component(part) || secret_filename(part));
        }
        return true;
    }
    if components.iter().any(|part| {
        matches!(
            part.as_str(),
            ".git"
                | ".ssh"
                | ".aws"
                | ".gnupg"
                | ".gcloud"
                | ".azure"
                | ".codelocal-beta"
                | ".codelocal"
                | "node_modules"
                | ".next"
                | "target"
                | ".venv"
                | "venv"
                | "__pycache__"
                | ".turbo"
        )
    }) {
        return true;
    }
    if components.iter().any(|part| !safe_component(part)) {
        return true;
    }
    let base = components.last().map(String::as_str).unwrap_or("");
    secret_filename(base)
}

fn secret_filename(base: &str) -> bool {
    if matches!(base, ".env.example" | ".env.sample" | ".env.template") {
        return false;
    }
    if base == ".env"
        || base.starts_with(".env.")
        || base.ends_with(".pem")
        || base.ends_with(".p12")
        || base.ends_with(".pfx")
        || base.ends_with(".key")
        || base.ends_with(".kdbx")
        || matches!(
            base,
            ".npmrc"
                | ".netrc"
                | ".pypirc"
                | ".dockerconfigjson"
                | "id_rsa"
                | "id_ed25519"
                | "id_ecdsa"
                | "authorized_keys"
                | "known_hosts"
                | "credentials"
                | "secrets"
                | "config.json"
        )
    {
        return true;
    }
    for stem in [
        "credential",
        "credentials",
        "service-account",
        "service_account",
        "private-key",
        "private_key",
        "secret",
        "secrets",
    ] {
        for extension in [".json", ".yaml", ".yml", ".toml", ".ini"] {
            if base == format!("{stem}{extension}") {
                return true;
            }
        }
    }
    false
}

fn safe_component(part: &str) -> bool {
    if matches!(part, ".env.example" | ".env.sample" | ".env.template") {
        return true;
    }
    !matches!(
        part,
        ".git"
            | ".ssh"
            | ".aws"
            | ".gnupg"
            | ".gcloud"
            | ".azure"
            | ".codelocal-beta"
            | ".codelocal"
            | "node_modules"
            | ".next"
            | "target"
            | ".venv"
            | "venv"
    ) && !part.starts_with(".env")
}

#[cfg(unix)]
fn open_beneath(root: &Path, segments: &[String]) -> Result<File> {
    use rustix::fs::{CWD, Mode, OFlags, openat};
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::DIRECTORY;
    let mut dir = openat(CWD, root, flags, Mode::empty()).map_err(io::Error::from)?;
    for component in &segments[..segments.len() - 1] {
        dir = openat(&dir, component.as_str(), flags, Mode::empty()).map_err(io::Error::from)?;
    }
    let final_flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    let fd = openat(
        &dir,
        segments[segments.len() - 1].as_str(),
        final_flags,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    Ok(File::from(fd))
}

#[cfg(not(unix))]
fn open_beneath(root: &Path, segments: &[String]) -> Result<File> {
    // Conservative Windows fallback for local-only manual reads. This does not
    // provide OS-level race-free guarantees; MUST NOT be used for remote MCP.
    let mut current = root.to_path_buf();
    for component in segments {
        current.push(component);
        let meta = fs::symlink_metadata(&current)?;
        if meta.file_type().is_symlink() {
            return Err(LocalReadError::Denied(
                "Windows symlink or reparse point forbidden",
            ));
        }
    }
    let canonical = fs::canonicalize(&current)?;
    if !canonical.starts_with(root) {
        return Err(LocalReadError::Denied("resolved path escaped workspace"));
    }
    Ok(File::open(canonical)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Registry, String) {
        let temp = tempfile::tempdir().unwrap();
        let registry = Registry::new(temp.path().join("state"));
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("hello.txt"), b"hello\r\nworld\r\n").unwrap();
        let entry = registry.grant(&project).unwrap();
        (temp, registry, entry.workspace_id)
    }

    #[test]
    fn file_info_and_line_range_preserve_go_dto() {
        let (_temp, registry, id) = setup();
        let reader = LocalReader::from_registry(&registry, &id).unwrap();
        assert_eq!(reader.workspace_id(), id);
        let info = reader.info("hello.txt").unwrap();
        let value = serde_json::to_value(&info).unwrap();
        assert_eq!(value["isFile"], true);
        assert_eq!(value["size"], 14);
        assert_eq!(value["path"], "hello.txt");
        assert_eq!(
            value["hash"],
            hex::encode(Sha256::digest(b"hello\r\nworld\r\n"))
        );
        let part =
            serde_json::to_value(reader.read("hello.txt", Some(2), Some(2)).unwrap()).unwrap();
        assert_eq!(part["content"], "world");
        assert_eq!(part["totalLines"], 2);
        assert_eq!(part["startLine"], 2);
        assert_eq!(part["endLine"], 2);
        assert_eq!(part["binary"], false);
    }

    #[test]
    fn denies_ungranted_workspace_and_sensitive_names() {
        let (temp, registry, id) = setup();
        assert!(LocalReader::from_registry(&registry, "unknown").is_err());
        let reader = LocalReader::from_registry(&registry, &id).unwrap();
        for path in [
            "../outside.txt",
            "/etc/passwd",
            "C:\\Windows\\win.ini",
            ".ssh/id_rsa",
            ".aws/credentials",
            ".git/config",
            ".env",
            ".env.local",
            "config/secrets.json",
            "service_account.json",
            "cert.pem",
            ".codelocal/credentials.json",
            ".codelocal/worktrees/current/main.rs",
            ".codelocal-beta/credential.json",
            "node_modules/pkg/index.js",
            "target/debug/output",
            "id_ed25519",
        ] {
            assert!(
                reader.read(path, None, None).is_err(),
                "accepted sensitive: {path}"
            );
        }
        fs::write(
            temp.path().join("project/.env.example"),
            b"FAKE_KEY=example",
        )
        .unwrap();
        assert!(reader.read(".env.example", None, None).is_ok());
        assert!(reader.read("hello.txt", Some(9), Some(1)).is_err());
        registry.revoke(&id).unwrap();
        assert!(LocalReader::from_registry(&registry, &id).is_err());
    }

    #[test]
    fn tampered_beta_registry_cannot_grant_credential_directory() {
        let temp = tempfile::tempdir().unwrap();
        let restricted = temp.path().join(".ssh");
        fs::create_dir(&restricted).unwrap();
        fs::write(restricted.join("config"), b"private").unwrap();
        let registry = Registry::new(temp.path().join("state"));
        fs::create_dir_all(registry.state_dir()).unwrap();
        let id = id_for_path(&fs::canonicalize(&restricted).unwrap());
        let fake_grant = serde_json::json!({
            "version": 1,
            "workspaces": [{
                "workspaceId": id,
                "workspaceName": ".ssh",
                "localPath": restricted,
                "grantedAt": 1
            }]
        });
        fs::write(
            registry.state_dir().join("workspaces.json"),
            serde_json::to_vec(&fake_grant).unwrap(),
        )
        .unwrap();
        assert!(LocalReader::from_registry(&registry, &id).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn prevents_symlink_escape_even_to_another_file_inside_workspace() {
        use std::os::unix::fs::symlink;
        let (temp, registry, id) = setup();
        let root = temp.path().join("project");
        let outside = temp.path().join("outside-secret.txt");
        fs::write(&outside, b"secret outside").unwrap();
        symlink(&outside, root.join("escape.txt")).unwrap();
        symlink(root.join("hello.txt"), root.join("linked.txt")).unwrap();
        symlink(temp.path(), root.join("up")).unwrap();
        let reader = LocalReader::from_registry(&registry, &id).unwrap();
        for file in ["escape.txt", "linked.txt", "up/outside-secret.txt"] {
            assert!(
                reader.read(file, None, None).is_err(),
                "symlink read: {file}"
            );
        }
    }

    #[test]
    fn bounded_reads_batch_and_binary() {
        let (temp, registry, id) = setup();
        let root = temp.path().join("project");
        fs::write(root.join("blob.dat"), [0, 1, 2, 3]).unwrap();
        fs::write(
            root.join("huge.txt"),
            vec![b'x'; (MAX_READ_BYTES + 1) as usize],
        )
        .unwrap();
        let reader = LocalReader::from_registry(&registry, &id).unwrap();
        let binary = serde_json::to_value(reader.read("blob.dat", None, None).unwrap()).unwrap();
        assert_eq!(binary["binary"], true);
        assert!(binary["content"].is_null());
        assert!(reader.read("huge.txt", Some(1), Some(2)).is_err());
        let input = vec!["hello.txt".into(), "blob.dat".into()];
        let batch = serde_json::to_value(reader.many(&input).unwrap()).unwrap();
        assert_eq!(batch["files"][0]["path"], "hello.txt");
        assert_eq!(batch["files"][1]["binary"], true);
        assert_eq!(batch["totalBytes"], 18);
        assert!(
            reader
                .many(&["hello.txt".into(), "huge.txt".into()])
                .is_err()
        );
    }

    #[test]
    fn portable_project_metadata_is_allowed_but_runtime_paths_are_hidden() {
        let (temp, registry, id) = setup();
        let portable = temp.path().join("project/.codelocal");
        fs::create_dir_all(portable.join("rules")).unwrap();
        fs::write(portable.join("project.json"), b"{}").unwrap();
        fs::write(portable.join("rules/agent.md"), b"trusted instructions").unwrap();
        fs::write(portable.join("rules/secrets.json"), b"private").unwrap();
        let reader = LocalReader::from_registry(&registry, &id).unwrap();
        assert!(reader.read(".codelocal/project.json", None, None).is_ok());
        assert!(reader.read(".codelocal/rules/agent.md", None, None).is_ok());
        assert!(reader.read(".codelocal/rules/.env", None, None).is_err());
        assert!(
            reader
                .read(".codelocal/rules/secrets.json", None, None)
                .is_err()
        );
        assert!(reader.read(".codelocal/tmp/trace.txt", None, None).is_err());
    }
}
