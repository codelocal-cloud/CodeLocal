//! Explicit beta public origin. Never inherit production's public URL.
pub fn beta_origin() -> Option<String> {
    let value = std::env::var("CODELOCAL_BETA_PUBLIC_BASE_URL").ok()?;
    validate(&value)
}

pub fn validate(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw.trim()).ok()?;
    let host = parsed.host_str()?;
    let local = matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]");
    if !(parsed.scheme() == "https" || (local && parsed.scheme() == "http"))
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return None;
    }
    Some(parsed.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn beta_origin_must_be_https_or_loopback_http() {
        assert_eq!(
            validate("https://beta.example.com"),
            Some("https://beta.example.com".into())
        );
        assert_eq!(
            validate("http://localhost:3100"),
            Some("http://localhost:3100".into())
        );
        assert!(validate("http://production.example.com").is_none());
        assert!(validate("https://user:secret@example.com").is_none());
        assert!(validate("https://example.com/path").is_none());
        assert!(validate("javascript:alert(1)").is_none());
    }
}
