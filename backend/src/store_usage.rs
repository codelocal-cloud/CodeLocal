// Beta PostgreSQL usage telemetry compatible with the Go dashboard DTO.
// A persisted account-scoped aggregate, not mock counters. Only trusted
// Rust backend components may record usage events.

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpUsageWindow {
    pub calls: i64,
    pub input_tokens_estimated: i64,
    pub output_tokens_estimated: i64,
    pub total_tokens_estimated: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReportedUsageWindow {
    pub turns: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FourWindows<T: Serialize> {
    pub last1h: T,
    pub last24h: T,
    pub last30d: T,
    pub all_time: T,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    pub estimated: bool,
    pub scope: &'static str,
    pub last1h: McpUsageWindow,
    pub last24h: McpUsageWindow,
    pub last30d: McpUsageWindow,
    pub all_time: McpUsageWindow,
    pub web_chat: ReportedWindows,
    pub mcp: EstimatedWindows,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportedWindows {
    pub source: &'static str,
    #[serde(flatten)]
    pub windows: FourWindows<ReportedUsageWindow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimatedWindows {
    pub source: &'static str,
    #[serde(flatten)]
    pub windows: FourWindows<McpUsageWindow>,
}

/// An actual MCP invocation measured by a trusted server component.
/// event_id is mandatory so repeated delivery cannot double count usage.
#[derive(Debug, Clone)]
pub struct McpUsageEvent<'a> {
    pub event_id: &'a str,
    pub user_id: &'a str,
    pub device_id: &'a str,
    pub workspace_id: &'a str,
    pub tool: &'a str,
    pub calls: i64,
    pub input_bytes: i64,
    pub output_bytes: i64,
    pub input_tokens_est: i64,
    pub output_tokens_est: i64,
    pub created_at: i64,
}

// Pending the model provider chat port; no fabricated provider usage.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ChatUsageEvent<'a> {
    pub event_id: &'a str,
    pub user_id: &'a str,
    pub thread_id: &'a str,
    pub model: &'a str,
    pub protocol: &'a str,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub created_at: i64,
}

fn valid_label(value: &str, max: usize) -> bool {
    !value.trim().is_empty() &&
        value.len() <= max && !value.chars().any(char::is_control)
}

#[allow(dead_code)]
fn valid_optional_label(value: &str, max: usize) -> bool {
    value.is_empty() || valid_label(value, max)
}

fn bucket_start(timestamp: i64) -> i64 {
    (timestamp / HOUR_MS) * HOUR_MS
}

impl AuthStore {
    /// Record one *actual* backend MCP usage event exactly once and roll up
    /// counts transactionally. There is no user-callable external mutation route.
    pub async fn record_mcp_usage(&self, event: &McpUsageEvent<'_>) -> Result<bool, StoreError> {
        let values = [event.calls, event.input_bytes, event.output_bytes,
            event.input_tokens_est, event.output_tokens_est];
        if !valid_label(event.event_id, 128)
            || !valid_label(event.user_id, 128)
            || !valid_label(event.tool, 128)
            || !valid_optional_label(event.device_id, 128)
            || !valid_optional_label(event.workspace_id, 128)
            || event.created_at <= 0
            || values.iter().any(|v| *v < 0)
            || event.calls == 0
        {
            return Err(unavailable("invalid beta MCP usage event"));
        }
        let mut client = self.pool.get().await
            .map_err(|_| unavailable("beta usage database unavailable"))?;
        let tx = client.transaction().await
            .map_err(|_| unavailable("beta usage transaction unavailable"))?;
        let inserted = tx.execute(
            "INSERT INTO codelocal_beta_mcp_usage_events(event_id,user_id,created_at)
              VALUES($1,$2,$3) ON CONFLICT(event_id) DO NOTHING",
            &[&event.event_id,&event.user_id,&event.created_at]
        ).await.map_err(|_| unavailable("beta MCP event deduplication failed"))?;
        if inserted == 0 {
            tx.commit().await.map_err(|_| unavailable("beta MCP deduplication commit failed"))?;
            return Ok(false);
        }
        let bucket = bucket_start(event.created_at);
        tx.execute(
            "INSERT INTO codelocal_mcp_usage (
               user_id,bucket_start,device_id,workspace_id,tool,calls,
               input_bytes,output_bytes,input_tokens_est,output_tokens_est
             ) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT(user_id,bucket_start,device_id,workspace_id,tool)
             DO UPDATE SET
               calls=codelocal_mcp_usage.calls+EXCLUDED.calls,
               input_bytes=codelocal_mcp_usage.input_bytes+EXCLUDED.input_bytes,
               output_bytes=codelocal_mcp_usage.output_bytes+EXCLUDED.output_bytes,
               input_tokens_est=codelocal_mcp_usage.input_tokens_est+EXCLUDED.input_tokens_est,
               output_tokens_est=codelocal_mcp_usage.output_tokens_est+EXCLUDED.output_tokens_est",
            &[&event.user_id,&bucket,&event.device_id,&event.workspace_id,&event.tool,
              &event.calls,&event.input_bytes,&event.output_bytes,
              &event.input_tokens_est,&event.output_tokens_est]
        ).await.map_err(|_| unavailable("beta MCP usage rollup failed"))?;
        tx.commit().await.map_err(|_| unavailable("beta MCP usage commit failed"))?;
        Ok(true)
    }

    /// Store only provider-reported token counts; never infer "reported"
    /// usage from model pricing or request content.
    #[allow(dead_code)]
    pub async fn record_chat_usage(&self, event: &ChatUsageEvent<'_>) -> Result<bool, StoreError> {
        if !valid_label(event.event_id,128)
            || !valid_label(event.user_id,128)
            || !valid_optional_label(event.thread_id,128)
            || !valid_label(event.model,128)
            || !valid_label(event.protocol,64)
            || event.created_at <= 0
            || event.input_tokens < 0 || event.output_tokens < 0
            || event.total_tokens < 0
            || event.total_tokens == 0
        {
            return Err(unavailable("invalid beta provider-reported chat usage event"));
        }
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta chat usage database unavailable"))?;
        let inserted = client.execute(
            "INSERT INTO codelocal_dashboard_chat_usage(
                event_id,user_id,thread_id,model,protocol,
                input_tokens,output_tokens,total_tokens,created_at)
             VALUES($1,$2,NULLIF($3,''),$4,$5,$6,$7,$8,$9)
             ON CONFLICT(event_id) DO NOTHING",
            &[&event.event_id,&event.user_id,&event.thread_id,&event.model,
              &event.protocol,&event.input_tokens,&event.output_tokens,
              &event.total_tokens,&event.created_at],
        ).await.map_err(|_| unavailable("beta chat usage insertion failed"))?;
        Ok(inserted == 1)
    }

    /// Only the authenticated account's own usage rows are aggregated.
    /// A zero window means there are no *recorded* events in that window,
    /// not that other unported usage sources have been counted.
    pub async fn usage_report(&self, user_id: &str) -> Result<UsageReport, StoreError> {
        if !valid_label(user_id,128) {
            return Err(unavailable("invalid usage account"));
        }
        let client = self.pool.get().await
            .map_err(|_| unavailable("beta usage database unavailable"))?;
        let now = now_millis();
        let since = [
            bucket_start(now - HOUR_MS),
            bucket_start(now - DAY_MS),
            bucket_start(now - 30 * DAY_MS),
            0_i64,
        ];
        let mut mcp_windows = Vec::with_capacity(4);
        let mut chat_windows = Vec::with_capacity(4);
        for (index, lower) in since.iter().enumerate() {
            let mcp = client.query_one(
                "SELECT COALESCE(SUM(calls),0)::bigint,
                        COALESCE(SUM(input_tokens_est),0)::bigint,
                        COALESCE(SUM(output_tokens_est),0)::bigint
                 FROM codelocal_mcp_usage
                 WHERE user_id=$1 AND ($2::bigint=0 OR bucket_start >= $2)",
                &[&user_id, lower]
            ).await.map_err(|_| unavailable("beta MCP usage query failed"))?;
            let input:i64 = mcp.get(1);
            let output:i64 = mcp.get(2);
            mcp_windows.push(McpUsageWindow {
                calls:mcp.get(0),input_tokens_estimated:input,
                output_tokens_estimated:output,
                total_tokens_estimated:input.saturating_add(output),
            });
            let actual_since = if index == 3 {0} else {
                match index {
                    0 => now - HOUR_MS,
                    1 => now - DAY_MS,
                    _ => now - 30 * DAY_MS,
                }
            };
            let chat = client.query_one(
                "SELECT COUNT(*)::bigint,
                        COALESCE(SUM(input_tokens),0)::bigint,
                        COALESCE(SUM(output_tokens),0)::bigint,
                        COALESCE(SUM(total_tokens),0)::bigint
                 FROM codelocal_dashboard_chat_usage
                 WHERE user_id=$1 AND ($2::bigint=0 OR created_at >= $2)",
                &[&user_id, &actual_since]
            ).await.map_err(|_| unavailable("beta chat usage query failed"))?;
            chat_windows.push(ReportedUsageWindow {
                turns:chat.get(0),input_tokens:chat.get(1),
                output_tokens:chat.get(2),total_tokens:chat.get(3),
            });
        }
        // Exactly four chronological windows are always produced; no source
        // can be silently swapped with mock defaults.
        let mcp = FourWindows {
            last1h:mcp_windows.remove(0),
            last24h:mcp_windows.remove(0),
            last30d:mcp_windows.remove(0),
            all_time:mcp_windows.remove(0),
        };
        let chat = FourWindows {
            last1h:chat_windows.remove(0),
            last24h:chat_windows.remove(0),
            last30d:chat_windows.remove(0),
            all_time:chat_windows.remove(0),
        };
        Ok(UsageReport {
            estimated:true,
            scope:"MCP payload only; not full AI model/provider billing tokens",
            last1h:mcp.last1h.clone(),
            last24h:mcp.last24h.clone(),
            last30d:mcp.last30d.clone(),
            all_time:mcp.all_time.clone(),
            mcp:EstimatedWindows {
                source:"payload_estimated",windows:mcp
            },
            web_chat:ReportedWindows {
                source:"provider_reported",windows:chat
            },
        })
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    #[test]
    fn bucket_starts_are_hour_aligned_and_events_validate_before_db() {
        assert_eq!(bucket_start(3_600_000),3_600_000);
        assert_eq!(bucket_start(3_600_001),3_600_000);
        assert!(!valid_label("has\nnewline",128));
        assert!(valid_optional_label("",128));
    }
}
