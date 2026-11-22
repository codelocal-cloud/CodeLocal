//! Resend email delivery compatible with old-project/internal/mailer/resend.go.
//! No development shortcut can expose one-time codes or claim a sent email.
use reqwest::Client;
use serde::Serialize;
use std::{env, time::Duration};

#[derive(Clone)]
pub struct Mailer {
    from: String,
    api_key: String,
    url: String,
    client: Client,
}

#[derive(Serialize)]
struct EmailMessage<'a> {
    from: &'a str,
    to: Vec<&'a str>,
    subject: &'a str,
    text: &'a str,
}

impl Mailer {
    pub fn from_env() -> Option<Self> {
        let api_key = env::var("CODELOCAL_BETA_RESEND_API_KEY")
            .ok()?
            .trim()
            .to_owned();
        let from = env::var("CODELOCAL_BETA_EMAIL_FROM")
            .ok()?
            .trim()
            .to_owned();
        if api_key.is_empty() || from.is_empty() {
            return None;
        }
        // For now deliberately hard-code the real provider: no arbitrary URL fetching,
        // no sending access tokens to a user-controllable destination.
        let client = Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .ok()?;
        Some(Self {
            api_key,
            from,
            url: "https://api.resend.com/emails".into(),
            client,
        })
    }

    pub async fn send(
        &self,
        to: &str,
        subject: &str,
        text: &str,
        idempotency: &str,
    ) -> Result<(), ()> {
        if to.len() > 254 || !to.contains('@') || subject.is_empty() || idempotency.is_empty() {
            return Err(());
        }
        let response = self
            .client
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .header("Idempotency-Key", idempotency)
            .json(&EmailMessage {
                from: &self.from,
                to: vec![to],
                subject,
                text,
            })
            .send()
            .await
            .map_err(|_| ())?;
        if !response.status().is_success() {
            return Err(());
        }
        Ok(())
    }
}
