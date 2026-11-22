//! Native Rust beta pairing flow. Uses only beta-specific state and API URL.
//! Does not claim that local daemon / remote MCP execution is implemented.
use crate::workspace::Registry;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use fs2::FileExt;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tempfile::NamedTempFile;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    credential_id: String,
    credential_secret: String,
    device_id: String,
    device_name: String,
    email: Option<String>,
    server_url: String,
    created_at: u64,
    #[serde(default)]
    device_public_key: String,
    #[serde(default)]
    device_private_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pairing {
    pairing_id: String,
    code: String,
    expires_at: u64,
    approve_url: String,
    retry_safe_claim: bool,
    device_signing: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Claimed {
    credential_id: String,
    device_id: String,
    device_name: String,
    email: Option<String>,
}

fn beta_url() -> Result<String, String> {
    let value = env::var("CODELOCAL_BETA_API_URL")
        .map_err(|_| "set CODELOCAL_BETA_API_URL explicitly to your isolated Rust beta API")?;
    let url = Url::parse(&value).map_err(|_| "invalid CODELOCAL_BETA_API_URL")?;
    let local = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
    );
    if (url.scheme() != "https" && !(local && url.scheme() == "http"))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(
            "beta API origin must be HTTPS, or loopback HTTP, with no path/credentials".into(),
        );
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn validate_approval_url(url: &str) -> Result<(), String> {
    let parsed = Url::parse(url).map_err(|_| "invalid approval URL")?;
    let local = matches!(
        parsed.host_str(),
        Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
    );
    if (parsed.scheme() != "https" && !(local && parsed.scheme() == "http"))
        || parsed.path() != "/pair/approve"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("unsafe pairing approval URL".into());
    }
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn random_hex(size: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; size];
    getrandom::fill(&mut bytes).map_err(|_| "unable to generate secure random bytes")?;
    Ok(hex::encode(bytes))
}

fn private_dir(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("beta identity directory cannot be a symlink".into());
    }
    fs::create_dir_all(path).map_err(|err| err.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn check_not_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        Err("beta identity file cannot be a symlink".into())
    } else {
        Ok(())
    }
}

fn private_permissions(file: &File) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn save_credential(dir: &Path, c: &Credential) -> Result<(), String> {
    private_dir(dir)?;
    let target = dir.join("credential.json");
    check_not_symlink(&target)?;
    let mut tmp = NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    private_permissions(tmp.as_file())?;
    serde_json::to_writer_pretty(tmp.as_file_mut(), c).map_err(|e| e.to_string())?;
    tmp.write_all(b"\n").map_err(|e| e.to_string())?;
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    tmp.persist(target).map_err(|e| e.error.to_string())?;
    #[cfg(unix)]
    File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_credential(dir: &Path) -> Result<Option<Credential>, String> {
    let target = dir.join("credential.json");
    check_not_symlink(&target)?;
    match fs::read(&target) {
        Ok(bytes) => {
            let credential: Credential = serde_json::from_slice(&bytes)
                .map_err(|_| "corrupt beta credential state; refusing overwrite")?;
            if !credential.credential_id.starts_with("cld_")
                || credential.credential_secret.len() != 80
            {
                return Err("malformed beta credential; refusing overwrite".into());
            }
            Ok(Some(credential))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn device_id(dir: &Path) -> Result<String, String> {
    private_dir(dir)?;
    let target = dir.join("device-id");
    check_not_symlink(&target)?;
    match fs::read_to_string(&target) {
        Ok(id) => {
            let id = id.trim();
            if id.len() < 16
                || id.len() > 80
                || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err("invalid persistent beta device ID".into());
            }
            return Ok(id.to_owned());
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    };
    let value = format!("rust-beta-{}", random_hex(16)?);
    let mut file = match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return device_id(dir),
        Err(e) => return Err(e.to_string()),
    };
    private_permissions(&file)?;
    file.write_all(value.as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok(value)
}

fn check_existing(client: &Client, origin: &str, credential: &Credential) -> Result<bool, String> {
    let response = client
        .post(format!("{origin}/api/client/auth/check"))
        .header("X-CodeLocal-Credential-Id", &credential.credential_id)
        .header(
            "Authorization",
            format!("Device {}", credential.credential_secret),
        )
        .json(&json!({}))
        .send()
        .map_err(|_| "unable to check existing beta credential; kept unchanged")?;
    if response.status().is_success() {
        return Ok(true);
    }
    if response.status().as_u16() == 401
        && response
            .headers()
            .get("X-CodeLocal-Auth-Check")
            .and_then(|value| value.to_str().ok())
            == Some("credential-v1")
    {
        return Ok(false);
    }
    Err(format!(
        "ambiguous credential check status {}; kept existing credential",
        response.status()
    ))
}

fn lock(dir: &Path) -> Result<File, String> {
    private_dir(dir)?;
    let file = dir.join("pair.rust-beta.lock");
    check_not_symlink(&file)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(file)
        .map_err(|e| e.to_string())?;
    private_permissions(&lock)?;
    lock.try_lock_exclusive()
        .map_err(|_| "another beta pairing process is already running")?;
    Ok(lock)
}

/// Pair with explicit browser approval; never touches the production Go token store.
pub fn login(force: bool) -> Result<(), String> {
    let registry = Registry::beta_default().map_err(|e| e.to_string())?;
    let dir = registry.state_dir().to_owned();
    let _held_lock = lock(&dir)?;
    let origin = beta_url()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "unable to create secure beta HTTP client")?;
    let bootstrap = client
        .get(format!("{origin}/api/v1/system/status"))
        .send()
        .map_err(|_| "Rust beta API could not be reached")?;
    if !bootstrap.status().is_success() {
        return Err("target is not a working Rust beta backend".into());
    }
    let identity: serde_json::Value = bootstrap
        .json()
        .map_err(|_| "invalid Rust beta backend identity response")?;
    if identity["service"] != "codelocal-api" || identity["state"] != "bootstrap" {
        return Err("refusing to pair against an unrecognized/non-beta CodeLocal API".into());
    }
    if let Some(existing) = load_credential(&dir)? {
        if existing.server_url != origin && !force {
            return Err("saved credential belongs to a different server; choose login --force to replace it".into());
        }
        if !force {
            match check_existing(&client, &origin, &existing)? {
                true => {
                    println!(
                        "✓ Rust beta device is already paired: {}",
                        existing.device_name
                    );
                    return Ok(());
                }
                false => {
                    return Err(
                        "saved beta credential is revoked/invalid; use login --force to pair again"
                            .into(),
                    );
                }
            }
        }
    }
    let device_id = device_id(&dir)?;
    let device_name = format!("CodeLocal Rust Beta {}", &device_id[10..18]);
    let response = client
        .post(format!("{origin}/pair/start"))
        .json(&json!({"deviceId":device_id,"deviceName":device_name}))
        .send()
        .map_err(|_| "beta pairing API unavailable")?;
    if !response.status().is_success() {
        return Err(format!(
            "beta pairing start rejected ({})",
            response.status()
        ));
    }
    let pair: Pairing = response
        .json()
        .map_err(|_| "invalid pairing start response")?;
    validate_approval_url(&pair.approve_url)?;
    if !pair.device_signing {
        return Err("beta API must require Ed25519 device signing".into());
    }
    if !pair.retry_safe_claim {
        return Err("beta server did not support retry-safe claims".into());
    }
    if pair.pairing_id.len() != 32 || pair.code.len() != 6 {
        return Err("invalid pairing ID/code from beta server".into());
    }

    let mut signing_seed = [0u8; 32];
    getrandom::fill(&mut signing_seed).map_err(|_| "secure key generation failed")?;
    let signing_key = SigningKey::from_bytes(&signing_seed);
    let device_public_key = URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes());
    let device_private_key = URL_SAFE_NO_PAD.encode(signing_seed);
    let credential_id = format!("cld_{}", random_hex(16)?);
    let credential_secret = random_hex(40)?;
    let hash = hex::encode(Sha256::digest(credential_secret.as_bytes()));
    let claim_body = json!({
        "pairingId":pair.pairing_id,
        "code":pair.code,
        "credentialId":credential_id,
        "credentialSecretHash":hash,
        "devicePublicKey":device_public_key,
    });
    println!("Approve this Rust beta device in your browser.");
    println!("Pairing code: {}", pair.code);
    println!("Approve: {}", pair.approve_url);
    println!("Waiting for approval (up to 10 minutes)...");
    let end = pair
        .expires_at
        .saturating_add(30_000)
        .min(now_millis().saturating_add(630_000));
    while now_millis() < end {
        thread::sleep(Duration::from_secs(1));
        let Ok(response) = client
            .post(format!("{origin}/pair/claim"))
            .json(&claim_body)
            .send()
        else {
            continue;
        };
        if response.status().is_success() {
            let claimed: Claimed = response
                .json()
                .map_err(|_| "invalid pairing claim response")?;
            if claimed.credential_id != credential_id || claimed.device_id != device_id {
                return Err("pair claim returned an unexpected credential/device".into());
            }
            let credential = Credential {
                credential_id,
                credential_secret,
                device_id,
                device_name: claimed.device_name,
                email: claimed.email,
                server_url: origin,
                created_at: now_millis(),
                device_public_key,
                device_private_key,
            };
            if !check_existing(&client, &credential.server_url, &credential)? {
                return Err("server has not validated newly claimed beta credential".into());
            }
            save_credential(&dir, &credential)?;
            println!("✓ Rust beta device paired: {}", credential.device_name);
            println!("Remote native runtime and MCP are NOT enabled yet.");
            return Ok(());
        }
        if !matches!(response.status().as_u16(), 409 | 429 | 503) {
            return Err(format!("beta pair claim rejected ({})", response.status()));
        }
    }
    Err("beta pairing expired without approval".into())
}

pub fn local_status() -> Result<bool, String> {
    let registry = Registry::beta_default().map_err(|e| e.to_string())?;
    Ok(load_credential(registry.state_dir())?.is_some())
}

fn canonical_device_request(
    method: &str,
    path: &str,
    timestamp: u64,
    nonce: &str,
    body: &[u8],
) -> Vec<u8> {
    let digest = hex::encode(Sha256::digest(body));
    format!(
        "{}\n{path}\n{timestamp}\n{nonce}\n{digest}",
        method.to_ascii_uppercase()
    )
    .into_bytes()
}

/// Push only beta workspace IDs/names, never machine filesystem paths.
pub fn sync() -> Result<(), String> {
    let registry = Registry::beta_default().map_err(|e| e.to_string())?;
    let origin = beta_url()?;
    let c =
        load_credential(registry.state_dir())?.ok_or("no paired beta device; run login first")?;
    if c.server_url != origin {
        return Err("saved credential belongs to another beta API".into());
    }
    let seed_raw = URL_SAFE_NO_PAD
        .decode(&c.device_private_key)
        .map_err(|_| "beta device lacks signing key; re-pair with login --force")?;
    let seed: [u8; 32] = seed_raw
        .try_into()
        .map_err(|_| "invalid signing key; re-pair with login --force")?;
    let signing_key = SigningKey::from_bytes(&seed);
    if URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes()) != c.device_public_key {
        return Err("beta public/private signing keys do not match".into());
    }
    let workspaces = registry.list().map_err(|e| e.to_string())?;
    if workspaces.len() > 100 {
        return Err("beta workspace sync supports at most 100 grants".into());
    }
    let catalog: Vec<_> = workspaces
        .iter()
        .map(|w| {
            json!({
                "workspaceId":w.workspace_id,
                "workspaceName":w.workspace_name,
                "managed":false,
                "systemApp":false
            })
        })
        .collect();
    let payload = serde_json::to_vec(&json!({
        "clientVersion": env!("CARGO_PKG_VERSION"),
        "workspaces": catalog
    }))
    .map_err(|_| "could not serialize beta workspaces")?;
    const PATH: &str = "/api/client/workspaces/sync";
    let mut nonce_bytes = [0u8; 18];
    getrandom::fill(&mut nonce_bytes).map_err(|_| "secure nonce generation failed")?;
    let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);
    let timestamp = now_millis() / 1000;
    let canonical = canonical_device_request("POST", PATH, timestamp, &nonce, &payload);
    let signature = URL_SAFE_NO_PAD.encode(signing_key.sign(&canonical).to_bytes());
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "unable to initialize beta HTTP client")?;
    let resp = client
        .post(format!("{origin}{PATH}"))
        .header("Content-Type", "application/json")
        .header("X-CodeLocal-Credential-Id", &c.credential_id)
        .header("Authorization", format!("Device {}", c.credential_secret))
        .header("X-CodeLocal-Signature-Timestamp", timestamp.to_string())
        .header("X-CodeLocal-Signature-Nonce", nonce)
        .header("X-CodeLocal-Signature", signature)
        .body(payload)
        .send()
        .map_err(|_| "beta workspace sync could not reach server")?;
    if !resp.status().is_success() {
        return Err(format!(
            "signed beta workspace sync failed ({})",
            resp.status()
        ));
    }
    let result: serde_json::Value = resp
        .json()
        .map_err(|_| "invalid signed beta workspace sync response")?;
    if result["synced"].as_u64() != Some(workspaces.len() as u64) {
        return Err("server acknowledged unexpected workspace count".into());
    }
    println!(
        "Synced {} beta workspace metadata records; paths remain local.",
        workspaces.len()
    );
    println!("Cloud metadata only; runtime execution remains disabled.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn beta_origin_does_not_accept_remote_plaintext_or_credentials() {
        assert!(
            validate_approval_url("https://beta.example.com/pair/approve?pairingId=123").is_ok()
        );
        assert!(validate_approval_url("http://127.0.0.1:3100/pair/approve?pairingId=123").is_ok());
        assert!(validate_approval_url("http://not-local.example/pair/approve").is_err());
        assert!(validate_approval_url("https://user:pass@example.com/pair/approve").is_err());
        assert!(validate_approval_url("https://beta.example.com/evil").is_err());
    }

    #[test]
    fn beta_state_persists_only_under_explicit_directory() {
        let dir = tempdir().unwrap();
        let device = device_id(dir.path()).unwrap();
        assert_eq!(device, device_id(dir.path()).unwrap());
        assert!(device.starts_with("rust-beta-"));
        let credential = Credential {
            credential_id: "cld_1234".into(),
            credential_secret: "a".repeat(80),
            device_id: device,
            device_name: "Beta Mac".into(),
            email: None,
            server_url: "http://127.0.0.1:8080".into(),
            created_at: 1,
            device_public_key: String::new(),
            device_private_key: String::new(),
        };
        save_credential(dir.path(), &credential).unwrap();
        assert!(load_credential(dir.path()).unwrap().is_some());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(dir.path().join("credential.json")).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn cli_ed25519_signature_uses_go_compatible_canonical_format() {
        use ed25519_dalek::Verifier;
        let signing = SigningKey::from_bytes(&[13u8; 32]);
        let nonce = URL_SAFE_NO_PAD.encode([42u8; 18]);
        let body = br#"{"hello":"world"}"#;
        let message = canonical_device_request(
            "POST",
            "/api/client/workspaces/sync",
            1_700_000_000,
            &nonce,
            body,
        );
        let digest = hex::encode(Sha256::digest(body));
        let expected = format!("POST\n/api/client/workspaces/sync\n1700000000\n{nonce}\n{digest}");
        assert_eq!(message, expected.as_bytes());
        let sig = signing.sign(&message);
        assert!(signing.verifying_key().verify(&message, &sig).is_ok());
        let tampered = canonical_device_request(
            "POST",
            "/api/client/workspaces/sync",
            1_700_000_000,
            &nonce,
            br#"{"hello":"altered"}"#,
        );
        assert!(signing.verifying_key().verify(&tampered, &sig).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_credential_file() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let target = dir.path().join("actual");
        fs::write(&target, b"redacted").unwrap();
        let alias = dir.path().join("credential.json");
        symlink(&target, &alias).unwrap();
        assert!(load_credential(dir.path()).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"redacted");
    }
}
