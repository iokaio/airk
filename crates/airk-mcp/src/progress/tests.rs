// SPDX-License-Identifier: Apache-2.0
use super::*;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, NumberOrString,
        ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
};

fn progress(part: u8, stage: AnswerStage, covered: u8) -> AnswerProgress {
    AnswerProgress {
        part,
        parts: 2,
        round: 1,
        stage,
        sub_needs_covered: covered,
        sub_needs: 3,
    }
}

struct Manual(Mutex<Instant>);
impl Manual {
    fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap();
        *now += by;
    }
}

fn reporter() -> (
    Arc<ProgressReporter>,
    Arc<Manual>,
    Arc<Mutex<Vec<ProgressNotificationParam>>>,
) {
    let clock = Arc::new(Manual(Mutex::new(Instant::now())));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let (time, log) = (clock.clone(), sent.clone());
    let reporter = ProgressReporter::with_sender(
        ProgressToken(NumberOrString::Number(7)),
        Arc::new(move |param| log.lock().unwrap().push(param)),
        Arc::new(move || *time.0.lock().unwrap()),
    );
    (reporter, clock, sent)
}

fn messages(sent: &Mutex<Vec<ProgressNotificationParam>>) -> Vec<String> {
    sent.lock()
        .unwrap()
        .iter()
        .map(|param| param.message.clone().unwrap())
        .collect()
}

#[test]
fn reports_are_rate_limited_and_the_latest_pending_one_is_sent_next() {
    let (reporter, clock, sent) = reporter();
    let sink = reporter.sink();
    sink(progress(1, AnswerStage::Plan, 0));
    sink(progress(1, AnswerStage::Gather, 0));
    sink(progress(1, AnswerStage::Coverage, 1));
    assert_eq!(
        messages(&sent),
        ["part 1/2, round 1, plan, sub-needs covered 0/3"]
    );
    // Within the interval a heartbeat keeps the pending report.
    clock.advance(Duration::from_millis(500));
    reporter.heartbeat();
    assert_eq!(sent.lock().unwrap().len(), 1);
    // After it, only the latest pending report is sent.
    clock.advance(Duration::from_millis(500));
    reporter.heartbeat();
    assert_eq!(
        messages(&sent)[1],
        "part 1/2, round 1, coverage, sub-needs covered 1/3"
    );
    // A report a full interval later goes out at once.
    clock.advance(MIN_PROGRESS_INTERVAL);
    sink(progress(2, AnswerStage::FollowUp, 2));
    let all = sent.lock().unwrap().clone();
    assert_eq!(all.len(), 3);
    assert_eq!(
        all[2].message.as_deref(),
        Some("part 2/2, round 1, follow_up, sub-needs covered 2/3")
    );
    // Progress strictly increases and every notification carries the token.
    for (index, param) in all.iter().enumerate() {
        assert_eq!(param.progress, (index + 1) as f64);
        assert_eq!(
            param.progress_token,
            ProgressToken(NumberOrString::Number(7))
        );
    }
}

#[test]
fn heartbeats_name_the_latest_stage_and_elapsed_time_only_when_quiet() {
    let (reporter, clock, sent) = reporter();
    // Nothing reported yet: the first heartbeat says the call is running.
    clock.advance(Duration::from_secs(1));
    reporter.heartbeat();
    assert_eq!(messages(&sent), ["Answer running; 1 s elapsed"]);
    // Quiet for less than the heartbeat interval: nothing more.
    clock.advance(HEARTBEAT_INTERVAL - Duration::from_secs(1));
    reporter.heartbeat();
    assert_eq!(sent.lock().unwrap().len(), 1);
    clock.advance(Duration::from_secs(1));
    reporter.report(AnswerProgress {
        part: 0,
        parts: 0,
        round: 0,
        stage: AnswerStage::Split,
        sub_needs_covered: 0,
        sub_needs: 0,
    });
    clock.advance(HEARTBEAT_INTERVAL);
    reporter.heartbeat();
    assert_eq!(
        messages(&sent)[1..],
        ["split".to_owned(), "split; 31 s elapsed".to_owned()]
    );
}

/// A server whose tool reports through the reporter, as the Answer handler
/// and the stdio bridge do.
#[derive(Clone)]
struct Server;
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    async fn call_tool(
        &self,
        _request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let reporter = ProgressReporter::from_request(&context.meta, context.peer.clone());
        let sent = reporter.is_some();
        if let Some(reporter) = &reporter {
            let sink = reporter.sink();
            reporter
                .while_running(async { sink(progress(1, AnswerStage::Coverage, 2)) })
                .await;
        }
        Ok(CallToolResult::success(vec![ContentBlock::text(sent.to_string())]).into())
    }
}

/// Raw JSON-RPC over the duplex, so the test controls `_meta.progressToken`
/// (an SDK client substitutes its own).
#[tokio::test]
async fn progress_reaches_an_mcp_client_on_its_own_token_and_never_without_one()
-> anyhow::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let _server = tokio::spawn(async move {
        Server.serve(server_io).await?.waiting().await?;
        anyhow::Ok(())
    });
    let (read, mut write) = tokio::io::split(client_io);
    let mut lines = BufReader::new(read).lines();
    let mut send = async |message: serde_json::Value| -> anyhow::Result<()> {
        write.write_all(format!("{message}\n").as_bytes()).await?;
        Ok(write.flush().await?)
    };
    let mut next = async || -> anyhow::Result<serde_json::Value> {
        let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
            .await??
            .ok_or_else(|| anyhow::anyhow!("server closed"))?;
        Ok(serde_json::from_str(&line)?)
    };
    send(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"1"}}})).await?;
    assert_eq!(next().await?["id"], 1);
    send(serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await?;
    send(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"airk_answer","arguments":{},"_meta":{"progressToken":"answer-1"}}})).await?;
    let mut progress = Vec::new();
    let reply = loop {
        let message = next().await?;
        if message["method"] == "notifications/progress" {
            progress.push(message);
        } else {
            break message;
        }
    };
    // The reply may overtake the ordered notification task; wait for it.
    if progress.is_empty() {
        progress.push(next().await?);
    }
    assert_eq!(reply["id"], 2);
    assert_eq!(reply["result"]["content"][0]["text"], "true");
    let params = &progress[0]["params"];
    assert_eq!(progress[0]["method"], "notifications/progress");
    assert_eq!(params["progressToken"], "answer-1");
    assert_eq!(params["progress"], 1.0);
    assert_eq!(
        params["message"],
        "part 1/2, round 1, coverage, sub-needs covered 2/3"
    );
    // No token: no reporter and no notification before the reply.
    send(serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"airk_answer","arguments":{}}})).await?;
    let reply = next().await?;
    assert_eq!(reply["id"], 3, "{reply}");
    assert_eq!(reply["result"]["content"][0]["text"], "false");
    Ok(())
}
