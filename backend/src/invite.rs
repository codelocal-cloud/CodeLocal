//! Real account-scoped Rust beta invite/referral endpoint for existing Next.js.
use crate::{
    auth::AuthConfig,
    auth_session::{error, identify},
    public_url,
    store::InvitedUser,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(crate) fn valid_referral_code(code: &str) -> bool {
    code == "MMON"
        || (code.len() == 6
            && code
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

fn mask_email(email: &str) -> String {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return "CodeLocal user".to_owned();
    };
    if local.is_empty() || domain.is_empty() {
        return "CodeLocal user".to_owned();
    }
    let visible = if local.chars().count() >= 4 { 2 } else { 1 };
    let prefix: String = local.chars().take(visible).collect();
    format!("{prefix}***@{domain}")
}

fn user_initial(email: &str) -> String {
    email
        .trim()
        .chars()
        .next()
        .map_or("C".into(), |c| c.to_uppercase().to_string())
}

fn build_invite(
    user: &crate::store::User,
    origin: &str,
    invited: Vec<InvitedUser>,
    online: &HashSet<String>,
) -> Value {
    let mut active = 0_usize;
    let members = invited
        .into_iter()
        .map(|entry| {
            let is_online = online.contains(&entry.id);
            if is_online {
                active += 1;
            }
            json!({
                "emailMasked":mask_email(&entry.email),
                "initial":user_initial(&entry.email),
                "joinedAt":entry.created_at,
                "status":if is_online {"runtime_online"} else {"offline"}
            })
        })
        .collect::<Vec<_>>();
    json!({
        "referralCode":user.referral_code,
        "invitedBy":user.referred_by_code.as_deref()
            .filter(|code| !code.is_empty()).unwrap_or("Root account"),
        "inviteLink":format!("{origin}/register?ref={}",user.referral_code),
        "directCount":members.len(),
        "activeCount":active,
        "members":members,
    })
}

pub async fn resource(State(config): State<AuthConfig>, headers: HeaderMap) -> Response {
    let Some(store) = &config.store else {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let (_, _, user) = match identify(store, &config, &headers).await {
        Ok(Some(identity)) => identity,
        Ok(None) => return error(StatusCode::UNAUTHORIZED, "unauthorized"),
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "identity_unavailable"),
    };
    let Some(origin) = public_url::beta_origin() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "public_beta_origin_unavailable",
        );
    };
    if !valid_referral_code(&user.referral_code) {
        return error(StatusCode::SERVICE_UNAVAILABLE, "invalid_beta_referral");
    }
    let invited = match store.invited_by_referral(&user.referral_code).await {
        Ok(items) => items,
        Err(_) => return error(StatusCode::SERVICE_UNAVAILABLE, "invite_unavailable"),
    };
    let ids = invited
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let online = match store.invited_user_runtime_online(&ids).await {
        Ok(online) => online,
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "invite_presence_unavailable",
            );
        }
    };
    let payload = build_invite(&user, &origin, invited, &online);
    let mut response = Json(payload).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn go_compatible_mask_initials_and_referral_validation() {
        assert_eq!(mask_email("alice@example.com"), "al***@example.com");
        assert_eq!(mask_email("bob@example.com"), "b***@example.com");
        assert_eq!(mask_email("invalid"), "CodeLocal user");
        assert_eq!(mask_email(" @example.com"), "CodeLocal user");
        assert_eq!(user_initial("alice@example.com"), "A");
        assert_eq!(user_initial(""), "C");
        assert!(valid_referral_code("ABC123"));
        assert!(valid_referral_code("MMON"));
        assert!(!valid_referral_code("abc123"));
        assert!(!valid_referral_code("ABC/23"));
        assert!(!valid_referral_code("SHORT"));
    }

    #[test]
    fn invite_never_leaks_real_email_or_invitee_ids() {
        let inviter = crate::store::User {
            id: "owner-private-id".into(),
            email: "owner@example.com".into(),
            password_hash: "hash".into(),
            password_salt: "salt".into(),
            password_changed_at: 0,
            security_version: 1,
            referral_code: "ABC123".into(),
            referred_by_code: None,
            created_at: 1,
        };
        let invited = vec![
            InvitedUser {
                id: "invitee-1".into(),
                email: "alice@example.com".into(),
                created_at: 123,
            },
            InvitedUser {
                id: "invitee-2".into(),
                email: "bob@example.com".into(),
                created_at: 100,
            },
        ];
        let online = HashSet::from(["invitee-1".to_owned()]);
        let resource = build_invite(&inviter, "https://beta.example.com", invited, &online);
        assert_eq!(resource["referralCode"], "ABC123");
        assert_eq!(resource["invitedBy"], "Root account");
        assert_eq!(
            resource["inviteLink"],
            "https://beta.example.com/register?ref=ABC123"
        );
        assert_eq!(resource["directCount"], 2);
        assert_eq!(resource["activeCount"], 1);
        assert_eq!(resource["members"][0]["status"], "runtime_online");
        assert_eq!(resource["members"][1]["status"], "offline");
        let raw = resource.to_string();
        assert!(!raw.contains("alice@example.com"));
        assert!(!raw.contains("owner@example.com"));
        assert!(!raw.contains("invitee-1"));
        assert!(!raw.contains("password"));
        assert!(!raw.contains("credential"));
    }

    #[tokio::test]
    async fn invite_requires_authenticated_session() {
        let response = crate::router()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/invite")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let raw = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["error"], "unauthorized");
        assert!(json.get("members").is_none());
    }
}
