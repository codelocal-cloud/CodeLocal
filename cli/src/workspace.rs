//! Isolated Rust-beta workspace registry, compatible with the Go JSON shape and ID algorithm.
//!
//! IMPORTANT: Never modify the stable Go ~/.codelocal/workspaces.json from this beta.
//! The beta uses ~/.codelocal-beta/workspaces.json or CODELOCAL_BETA_STATE_DIR.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub enum RegistryError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidPath(String),
    ManagedWorkspace,
    InvalidRegistryVersion(u32),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "invalid workspace registry JSON: {e}"),
            Self::InvalidPath(e) => write!(f, "{e}"),
            Self::ManagedWorkspace => write!(
                f,
                "managed CodeLocal app cannot be removed as a normal workspace"
            ),
            Self::InvalidRegistryVersion(v) => {
                write!(f, "unsupported workspace registry version: {v}")
            }
        }
    }
}
impl Error for RegistryError {}
impl From<io::Error> for RegistryError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for RegistryError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
type Result<T> = std::result::Result<T, RegistryError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub workspace_id: String,
    pub workspace_name: String,
    pub local_path: PathBuf,
    pub granted_at: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub last_activated_at: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub system: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub system_app: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub managed: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Serialize, Deserialize)]
struct FileState {
    version: u32,
    workspaces: Vec<Workspace>,
}

pub struct Registry {
    file: PathBuf,
}

impl Registry {
    /// Choose a beta-only location. Legacy Go state must not be written by an incomplete client.
    pub fn beta_default() -> Result<Self> {
        let base = match env::var_os("CODELOCAL_BETA_STATE_DIR") {
            Some(base) if !base.is_empty() => PathBuf::from(base),
            _ => {
                let home = env::var_os("HOME")
                    .or_else(|| env::var_os("USERPROFILE"))
                    .ok_or_else(|| {
                        RegistryError::InvalidPath("home directory unavailable".into())
                    })?;
                PathBuf::from(home).join(".codelocal-beta")
            }
        };
        Ok(Self::new(base))
    }

    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            file: state_dir.into().join("workspaces.json"),
        }
    }

    pub fn state_dir(&self) -> &Path {
        self.file.parent().expect("workspace registry has a parent")
    }

    fn read(&self) -> Result<FileState> {
        // Reject replacing registry file with a symlink to an arbitrary user file.
        match fs::symlink_metadata(&self.file) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(RegistryError::InvalidPath(
                    "registry file cannot be a symlink".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(FileState {
                    version: 1,
                    workspaces: Vec::new(),
                });
            }
            Err(error) => return Err(error.into()),
        };
        let data: FileState = serde_json::from_reader(BufReader::new(File::open(&self.file)?))?;
        if data.version != 1 {
            return Err(RegistryError::InvalidRegistryVersion(data.version));
        }
        Ok(data)
    }

    pub fn list(&self) -> Result<Vec<Workspace>> {
        Ok(self
            .read()?
            .workspaces
            .into_iter()
            .filter(|ws| ws.local_path.is_dir())
            .collect())
    }

    pub fn grant(&self, path: impl AsRef<Path>) -> Result<Workspace> {
        let real = fs::canonicalize(path.as_ref())?;
        if !real.is_dir() {
            return Err(RegistryError::InvalidPath(
                "only directories can be granted as CodeLocal workspaces".into(),
            ));
        }
        validate_workspace_path(&real)?;
        let id = id_for_path(&real);
        self.with_lock(|| {
            let mut state = self.read()?;
            let existing = state
                .workspaces
                .iter()
                .find(|ws| ws.workspace_id == id)
                .cloned();
            let name = real
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("workspace");
            let out = Workspace {
                workspace_id: id.clone(),
                workspace_name: name.chars().take(120).collect(),
                local_path: real.clone(),
                granted_at: existing
                    .as_ref()
                    .map_or_else(now_millis, |ws| ws.granted_at),
                last_activated_at: existing.as_ref().map_or(0, |ws| ws.last_activated_at),
                system: false,
                system_app: false,
                managed: false,
                hidden: false,
            };
            state.workspaces.retain(|ws| ws.workspace_id != id);
            state.workspaces.push(out.clone());
            state
                .workspaces
                .sort_by(|a, b| a.workspace_name.cmp(&b.workspace_name));
            self.write(&state)?;
            Ok(out)
        })
    }

    /// Removes only the beta registry authorization, never the underlying directory.
    pub fn revoke(&self, identifier: &str) -> Result<bool> {
        if identifier.is_empty() {
            return Err(RegistryError::InvalidPath(
                "workspace path or ID required".into(),
            ));
        }
        let real = fs::canonicalize(identifier).ok();
        self.with_lock(|| {
            let mut state = self.read()?;
            let matches = |ws: &Workspace| {
                ws.workspace_id == identifier || real.as_deref() == Some(ws.local_path.as_path())
            };
            if state.workspaces.iter().any(|ws| matches(ws) && ws.managed) {
                return Err(RegistryError::ManagedWorkspace);
            }
            let previous = state.workspaces.len();
            state.workspaces.retain(|ws| !matches(ws));
            if state.workspaces.len() == previous {
                return Ok(false);
            }
            self.write(&state)?;
            Ok(true)
        })
    }

    fn with_lock<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        ensure_private_dir(self.state_dir())?;
        let lock_path = self.state_dir().join("workspaces.rust-beta.lock");
        if fs::symlink_metadata(&lock_path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(RegistryError::InvalidPath(
                "workspace lock cannot be a symlink".into(),
            ));
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        set_private_file_permissions(&lock)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match lock.try_lock_exclusive() {
                Ok(()) => break,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(RegistryError::InvalidPath(
                            "timed out waiting for the Rust beta workspace registry lock".into(),
                        ));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error.into()),
            }
        }
        let result = action();
        let unlock = FileExt::unlock(&lock);
        match (result, unlock) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error.into()),
        }
    }

    fn write(&self, state: &FileState) -> Result<()> {
        ensure_private_dir(self.state_dir())?;
        let mut tmp = NamedTempFile::new_in(self.state_dir())?;
        set_private_file_permissions(tmp.as_file())?;
        serde_json::to_writer_pretty(tmp.as_file_mut(), state)?;
        tmp.write_all(b"\n")?;
        tmp.as_file().sync_all()?;
        tmp.persist(&self.file).map_err(|error| error.error)?;
        #[cfg(unix)]
        File::open(self.state_dir())?.sync_all()?;
        Ok(())
    }
}

/// Deliberately mirrors Go workspace.IDForPath: slug[0:48] + "-" + sha256(canonical-path)[0:10].
pub fn id_for_path(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let mut slug = String::new();
    let mut last_was_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            slug.push(c);
            last_was_dash = c == '-';
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "workspace" } else { slug };
    let slug = &slug[..slug.len().min(48)];
    let hash = Sha256::digest(path.to_string_lossy().as_bytes());
    let short_hash = hash[..5]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{slug}-{short_hash}")
}

fn validate_workspace_path(path: &Path) -> Result<()> {
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .and_then(|home| fs::canonicalize(home).ok());
    if path.parent().is_none() || home.as_deref() == Some(path) {
        return Err(RegistryError::InvalidPath(
            "refusing to grant the filesystem root or entire home directory".into(),
        ));
    }
    if path.components().any(|component| {
        matches!(component, std::path::Component::Normal(name)
            if matches!(name.to_string_lossy().to_ascii_lowercase().as_str(),
                ".ssh" | ".aws" | ".gnupg" | ".gcloud" | ".azure"))
    }) {
        return Err(RegistryError::InvalidPath(
            "credential/config directories cannot be granted".into(),
        ));
    }
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(RegistryError::InvalidPath(
            "state directory cannot be a symlink".into(),
        ));
    }
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn set_private_file_permissions(file: &File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn id_is_stable_and_matches_go_hash() {
        let value = id_for_path(Path::new("/tmp/my project"));
        assert_eq!(value, "my-project-4cae6444ef");
        assert_eq!(value.len(), "my-project-".len() + 10);
        assert_eq!(id_for_path(Path::new("/tmp/my project")), value);
        assert_ne!(id_for_path(Path::new("/tmp/other")), value);
    }

    #[test]
    fn grant_list_revoke_persist_compatible_json() {
        let temp = tempdir().unwrap();
        let registry = Registry::new(temp.path().join("beta"));
        let project = temp.path().join("demo");
        fs::create_dir(&project).unwrap();

        let first = registry.grant(&project).unwrap();
        assert_eq!(first.workspace_name, "demo");
        assert_eq!(first.local_path, fs::canonicalize(&project).unwrap());
        let again = registry.grant(&project).unwrap();
        assert_eq!(first.granted_at, again.granted_at);
        assert_eq!(registry.list().unwrap().len(), 1);
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&registry.file).unwrap()).unwrap();
        assert_eq!(saved["version"], 1);
        assert_eq!(saved["workspaces"][0]["workspaceId"], first.workspace_id);
        assert_eq!(
            saved["workspaces"][0]["localPath"],
            first.local_path.to_str().unwrap()
        );
        assert_eq!(saved["workspaces"][0]["grantedAt"], first.granted_at);
        assert!(!registry.revoke("unknown").unwrap());
        assert!(registry.revoke(&first.workspace_id).unwrap());
        assert!(registry.list().unwrap().is_empty());
        assert!(project.exists(), "ungrant never deletes project files");
    }

    #[test]
    fn reject_credential_directory_and_root() {
        let tmp = tempdir().unwrap();
        let registry = Registry::new(tmp.path().join("beta"));
        let ssh = tmp.path().join(".ssh").join("project");
        fs::create_dir_all(&ssh).unwrap();
        assert!(registry.grant(ssh).is_err());
        #[cfg(unix)]
        assert!(registry.grant("/").is_err());
    }

    #[test]
    fn managed_workspace_cannot_be_revoked() {
        let tmp = tempdir().unwrap();
        let registry = Registry::new(tmp.path().join("beta"));
        let project = tmp.path().join("project");
        fs::create_dir(&project).unwrap();
        let mut entry = registry.grant(&project).unwrap();
        entry.managed = true;
        registry
            .write(&FileState {
                version: 1,
                workspaces: vec![entry.clone()],
            })
            .unwrap();
        assert!(matches!(
            registry.revoke(&entry.workspace_id),
            Err(RegistryError::ManagedWorkspace)
        ));
        assert_eq!(registry.list().unwrap().len(), 1);
    }

    #[test]
    fn corrupt_and_incompatible_state_never_gets_overwritten() {
        let tmp = tempdir().unwrap();
        let registry = Registry::new(tmp.path().join("beta"));
        let project = tmp.path().join("project");
        fs::create_dir(&project).unwrap();
        ensure_private_dir(registry.state_dir()).unwrap();
        fs::write(&registry.file, b"corrupt").unwrap();
        assert!(registry.grant(&project).is_err());
        assert_eq!(fs::read(&registry.file).unwrap(), b"corrupt");
        fs::write(&registry.file, b"{\"version\":42,\"workspaces\":[]}").unwrap();
        assert!(matches!(
            registry.grant(&project),
            Err(RegistryError::InvalidRegistryVersion(42))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_alias_dedupes_and_sensitive_target_is_blocked() {
        use std::os::unix::fs::symlink;
        let tmp = tempdir().unwrap();
        let registry = Registry::new(tmp.path().join("beta"));
        let project = tmp.path().join("project");
        fs::create_dir(&project).unwrap();
        let alias = tmp.path().join("alias");
        symlink(&project, &alias).unwrap();
        assert_eq!(
            registry.grant(&project).unwrap().workspace_id,
            registry.grant(&alias).unwrap().workspace_id
        );
        assert_eq!(registry.list().unwrap().len(), 1);
        let ssh = tmp.path().join(".ssh");
        fs::create_dir(&ssh).unwrap();
        let credential_alias = tmp.path().join("credentials");
        symlink(&ssh, &credential_alias).unwrap();
        assert!(registry.grant(credential_alias).is_err());
    }
}
