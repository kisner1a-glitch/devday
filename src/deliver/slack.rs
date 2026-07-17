use crate::model::Report;

/// Executive digest by default; verbose includes the full worked-on list.
pub fn format_digest(report: &Report, verbose: bool) -> String {
    let mut s = String::new();
    s.push_str("*devday status*\n");
    match &report.summary {
        Some(sum) => s.push_str(&format!("{sum}\n")),
        None => s.push_str(&format!(
            "{} item(s), {} blocker(s).\n",
            report.worked_on.len(),
            report.blockers.len()
        )),
    }
    if !report.blockers.is_empty() {
        s.push_str("\n*Blockers*\n");
        for b in &report.blockers {
            s.push_str(&format!("• {}\n", b.reason));
        }
    }
    if verbose {
        s.push_str("\n*Worked on*\n");
        for w in &report.worked_on {
            s.push_str(&format!("• {w}\n"));
        }
    }
    if !report.source_links.is_empty() {
        s.push_str("\n*Links*\n");
        for l in report
            .source_links
            .iter()
            .take(if verbose { usize::MAX } else { 5 })
        {
            s.push_str(&format!("• {l}\n"));
        }
    }
    s
}

pub async fn post_webhook(url: &str, text: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        // reqwest's transport-error Display embeds the request URL, and the
        // webhook URL itself carries the Slack secret — scrub it so it never
        // reaches stdout/logs on failure.
        .map_err(|e| e.to_string().replace(url, "[REDACTED]"))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("slack webhook HTTP {}", resp.status()))
    }
}

pub async fn post_bot(
    api_base: &str,
    token: &str,
    channel: &str,
    text: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_base}/chat.postMessage"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "channel": channel, "text": text }))
        .send()
        .await
        // Defense in depth: scrub the bot token from any transport-error text
        // (api_base/channel are not secrets, but never echo the token).
        .map_err(|e| e.to_string().replace(token, "[REDACTED]"))?;
    #[derive(serde::Deserialize)]
    struct Ack {
        ok: bool,
        #[serde(default)]
        error: Option<String>,
    }
    let ack: Ack = resp.json().await.map_err(|e| e.to_string())?;
    if ack.ok {
        Ok(())
    } else {
        Err(format!("slack: {}", ack.error.unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockerKind, BlockerSignal, Confidence};
    use chrono::Utc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn report() -> Report {
        Report {
            window_start: Utc::now(),
            window_end: Utc::now(),
            groups: vec![],
            summary: None,
            worked_on: vec!["did a thing".into()],
            next_up: vec![],
            blockers: vec![BlockerSignal {
                kind: BlockerKind::Explicit,
                reason: "CI red".into(),
                source_item_url: None,
                confidence: Confidence::High,
            }],
            source_links: vec![],
            generation_warnings: vec![],
        }
    }

    #[test]
    fn digest_includes_blockers() {
        let d = format_digest(&report(), false);
        assert!(d.contains("Blockers"));
        assert!(d.contains("CI red"));
    }

    #[test]
    fn verbose_adds_worked_on() {
        assert!(format_digest(&report(), true).contains("did a thing"));
        assert!(!format_digest(&report(), false).contains("did a thing"));
    }

    #[tokio::test]
    async fn webhook_post_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        assert!(post_webhook(&server.uri(), "hi").await.is_ok());
    }

    #[tokio::test]
    async fn bot_post_reports_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat.postMessage"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({ "ok": false, "error": "channel_not_found" }),
                ),
            )
            .mount(&server)
            .await;
        let e = post_bot(&server.uri(), "t", "#x", "hi").await.unwrap_err();
        assert!(e.contains("channel_not_found"));
    }
}
