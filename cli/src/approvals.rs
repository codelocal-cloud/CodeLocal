//! Beta-only, human-reviewed local MCP read approval inbox.
//! An MCP stdio client may request a read, but cannot approve it through MCP.
//! Exact tool + complete canonical JSON arguments + process session are bound
//! to an expiring one-use ticket, kept in a private, atomic local state file.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;

const MAX_PENDING: usize = 32;
const MAX_ARGUMENT_BYTES: usize = 512;
const MAX_STORED_BYTES: u64 = 128 * 1024;
const TICKET_TTL_MS: u64 = 5 * 60 * 1000;
const APPROVED_TTL_MS: u64 = 2 * 60 * 1000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Approved,
    Denied,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Ticket {
    id: String,
    session: String,
    tool: String,
    workspace_id: String,
    arguments: String,
    digest: String,
    status: Status,
    created_at: u64,
    expires_at: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalSummary {
    pub id: String,
    pub tool: String,
    pub workspace_id: String,
    pub arguments: String,
    pub status: Status,
    pub created_at: u64,
    pub expires_at: u64,
}

impl From<&Ticket> for ApprovalSummary {
    fn from(ticket: &Ticket) -> Self {
        Self {
            id: ticket.id.clone(),
            tool: ticket.tool.clone(),
            workspace_id: ticket.workspace_id.clone(),
            arguments: ticket.arguments.clone(),
            status: ticket.status.clone(),
            created_at: ticket.created_at,
            expires_at: ticket.expires_at,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    tickets: Vec<Ticket>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            tickets: Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Pending { id: String, expires_at: u64 },
    Denied,
}

pub struct ApprovalStore {
    root: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn valid_ticket_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn private_dir(dir: &Path) -> Result<(), String> {
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("approval state directory is a symlink".into());
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn private_file(file: &File) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn no_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        Err("approval state file cannot be a symlink".into())
    } else {
        Ok(())
    }
}

impl ApprovalStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn file(&self) -> PathBuf {
        self.root.join("mcp-approvals.json")
    }

    fn read(&self) -> Result<State, String> {
        let path = self.file();
        no_symlink(&path)?;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(State::default()),
            Err(err) => return Err(err.to_string()),
        };
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > MAX_STORED_BYTES {
            return Err("approval state is not a valid bounded file".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_STORED_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_STORED_BYTES {
            return Err("approval state exceeds size limit".into());
        }
        let data: State = serde_json::from_slice(&bytes)
            .map_err(|_| "approval state corrupt; refusing unsafe recovery")?;
        if data.version != 1 || data.tickets.len() > MAX_PENDING {
            return Err("approval state version/count invalid".into());
        }
        Ok(data)
    }

    fn write(&self, state: &State) -> Result<(), String> {
        let path = self.file();
        no_symlink(&path)?;
        let mut tmp = NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
        private_file(tmp.as_file())?;
        serde_json::to_writer_pretty(tmp.as_file_mut(), state).map_err(|e| e.to_string())?;
        tmp.as_file().sync_all().map_err(|e| e.to_string())?;
        tmp.persist(path).map_err(|e| e.error.to_string())?;
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn transaction<T>(
        &self,
        operation: impl FnOnce(&mut State) -> Result<T, String>,
    ) -> Result<T, String> {
        // Unix only: do not claim secure ACL semantics on other platforms yet.
        #[cfg(not(unix))]
        return Err("MCP approval persistence requires a secure Windows ACL implementation".into());
        private_dir(&self.root)?;
        let lock_file = self.root.join("mcp-approvals.lock");
        no_symlink(&lock_file)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_file)
            .map_err(|e| e.to_string())?;
        private_file(&lock)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match lock.try_lock_exclusive() {
                Ok(()) => break,
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err("MCP approval file lock timeout".into());
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(err) => return Err(err.to_string()),
            }
        }
        let mut state = self.read()?;
        let result = operation(&mut state);
        let saved = if result.is_ok() {
            self.write(&state)
        } else {
            Ok(())
        };
        let released = FileExt::unlock(&lock).map_err(|e| e.to_string());
        match (result, saved, released) {
            (Ok(value), Ok(()), Ok(())) => Ok(value),
            (Err(err), _, _) | (_, Err(err), _) | (_, _, Err(err)) => Err(err),
        }
    }

    pub fn list(&self) -> Result<Vec<ApprovalSummary>, String> {
        self.transaction(|state| {
            let now = now_ms();
            state.tickets.retain(|t| t.expires_at > now);
            Ok(state.tickets.iter().map(ApprovalSummary::from).collect())
        })
    }

    pub fn decide(&self, id: &str, approved: bool) -> Result<bool, String> {
        if !valid_ticket_id(id) {
            return Err("invalid approval ID".into());
        }
        self.transaction(|state| {
            let now = now_ms();
            let Some(item) = state
                .tickets
                .iter_mut()
                .find(|item| item.id == id && item.expires_at > now)
            else {
                return Ok(false);
            };
            if item.status != Status::Pending {
                return Ok(false);
            };
            item.status = if approved {
                Status::Approved
            } else {
                Status::Denied
            };
            item.expires_at = now + APPROVED_TTL_MS;
            Ok(true)
        })
    }

    pub fn check_or_queue(
        &self,
        session: &str,
        tool: &str,
        args: &Value,
        ticket_id: Option<&str>,
        workspace_id: &str,
    ) -> Result<Decision, String> {
        if session.len() != 32 || !session.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid MCP process session ID".into());
        }
        if !matches!(
            tool,
            "codelocal_local_info" | "codelocal_local_read" | "codelocal_local_search"
        ) {
            return Err("unknown or unapprovable MCP tool".into());
        }
        let serialized =
            serde_json::to_string(args).map_err(|_| "could not encode MCP arguments")?;
        if serialized.len() > MAX_ARGUMENT_BYTES {
            return Err("MCP read arguments exceed 512-byte approval preview".into());
        }
        let digest = hex::encode(Sha256::digest(
            format!("{session}\0{tool}\0{serialized}").as_bytes(),
        ));
        if ticket_id.is_some_and(|id| !valid_ticket_id(id)) {
            return Ok(Decision::Denied);
        }
        self.transaction(|state| {
            let now = now_ms();
            state.tickets.retain(|t| t.expires_at > now);
            if let Some(id) = ticket_id {
                let Some(index) = state.tickets.iter().position(|t| t.id == id) else {
                    return Ok(Decision::Denied);
                };
                let item = &state.tickets[index];
                if item.session != session
                    || item.digest != digest
                    || item.tool != tool
                    || item.workspace_id != workspace_id
                {
                    return Ok(Decision::Denied);
                }
                let answer = match item.status {
                    Status::Pending => Decision::Pending {
                        id: item.id.clone(),
                        expires_at: item.expires_at,
                    },
                    Status::Approved => Decision::Allowed,
                    Status::Denied => Decision::Denied,
                };
                if !matches!(answer, Decision::Pending { .. }) {
                    state.tickets.remove(index); // one-use, consumed before read
                }
                return Ok(answer);
            }
            if let Some(existing) = state.tickets.iter().find(|item| {
                item.session == session
                    && item.digest == digest
                    && item.tool == tool
                    && item.workspace_id == workspace_id
            }) {
                return Ok(Decision::Pending {
                    id: existing.id.clone(),
                    expires_at: existing.expires_at,
                });
            }
            if state.tickets.len() >= MAX_PENDING {
                return Err("MCP approval inbox full; review or deny pending requests".into());
            }
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(|_| "secure MCP approval ID unavailable")?;
            let id = hex::encode(random);
            let expires_at = now + TICKET_TTL_MS;
            state.tickets.push(Ticket {
                id: id.clone(),
                session: session.into(),
                tool: tool.into(),
                workspace_id: workspace_id.into(),
                arguments: serialized,
                digest,
                status: Status::Pending,
                created_at: now,
                expires_at,
            });
            Ok(Decision::Pending { id, expires_at })
        })
    }
}

pub fn fresh_session() -> Result<String, String> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "secure MCP session ID unavailable")?;
    Ok(hex::encode(random))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn session() -> String {
        "a".repeat(32)
    }

    #[test]
    fn approval_binds_exact_request_and_session_and_is_one_use() {
        let root = tempdir().unwrap();
        let store = ApprovalStore::new(root.path());
        let args = serde_json::json!({"workspaceId":"repo-1","path":"src/lib.rs"});
        let ticket = store
            .check_or_queue(&session(), "codelocal_local_read", &args, None, "repo-1")
            .unwrap();
        let Decision::Pending { id, expires_at } = ticket else {
            panic!("must require human approval")
        };
        assert!(expires_at > now_ms());
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(
            store
                .check_or_queue(
                    &"b".repeat(32),
                    "codelocal_local_read",
                    &args,
                    Some(&id),
                    "repo-1"
                )
                .unwrap(),
            Decision::Denied
        );
        assert_eq!(
            store
                .check_or_queue(
                    &session(),
                    "codelocal_local_read",
                    &serde_json::json!({"workspaceId":"repo-1","path":"private.rs"}),
                    Some(&id),
                    "repo-1"
                )
                .unwrap(),
            Decision::Denied
        );
        assert!(store.decide(&id, true).unwrap());
        assert!(!store.decide(&id, true).unwrap());
        assert_eq!(
            store
                .check_or_queue(
                    &session(),
                    "codelocal_local_read",
                    &args,
                    Some(&id),
                    "repo-1"
                )
                .unwrap(),
            Decision::Allowed
        );
        assert_eq!(
            store
                .check_or_queue(
                    &session(),
                    "codelocal_local_read",
                    &args,
                    Some(&id),
                    "repo-1"
                )
                .unwrap(),
            Decision::Denied
        );
    }

    #[test]
    fn denied_and_invalid_tickets_never_grant_access() {
        let root = tempdir().unwrap();
        let store = ApprovalStore::new(root.path());
        let args = serde_json::json!({"workspaceId":"beta","query":"main","mode":"text"});
        let Decision::Pending { id, .. } = store
            .check_or_queue(&session(), "codelocal_local_search", &args, None, "beta")
            .unwrap()
        else {
            panic!("pending required")
        };
        assert!(store.decide(&id, false).unwrap());
        assert_eq!(
            store
                .check_or_queue(
                    &session(),
                    "codelocal_local_search",
                    &args,
                    Some(&id),
                    "beta"
                )
                .unwrap(),
            Decision::Denied
        );
        assert_eq!(
            store
                .check_or_queue(
                    &session(),
                    "codelocal_local_search",
                    &args,
                    Some("not-valid"),
                    "beta"
                )
                .unwrap(),
            Decision::Denied
        );
        assert!(
            store
                .check_or_queue(
                    &session(),
                    "codelocal_local_search",
                    &serde_json::json!({"long":"x".repeat(1000)}),
                    None,
                    "beta"
                )
                .is_err()
        );
    }

    #[test]
    fn expired_ticket_never_authorizes_even_if_previously_approved() {
        let root = tempdir().unwrap();
        let store = ApprovalStore::new(root.path());
        let args = serde_json::json!({"workspaceId":"beta","path":"main.rs"});
        let Decision::Pending { id, .. } = store
            .check_or_queue(&session(), "codelocal_local_read", &args, None, "beta")
            .unwrap()
        else {
            panic!("ticket required")
        };
        assert!(store.decide(&id, true).unwrap());
        store
            .transaction(|state| {
                let ticket = state
                    .tickets
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or("missing test ticket")?;
                ticket.expires_at = now_ms().saturating_sub(1);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store
                .check_or_queue(&session(), "codelocal_local_read", &args, Some(&id), "beta")
                .unwrap(),
            Decision::Denied
        );
        assert!(store.list().unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn approval_file_private_and_symlink_protected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = tempdir().unwrap();
        let store = ApprovalStore::new(root.path());
        let _ = store.list().unwrap();
        let metadata = fs::metadata(root.path().join("mcp-approvals.json")).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let target = root.path().join("untouched");
        fs::write(&target, "private").unwrap();
        fs::remove_file(root.path().join("mcp-approvals.json")).unwrap();
        symlink(&target, root.path().join("mcp-approvals.json")).unwrap();
        assert!(store.list().is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "private");
    }
}
