//! An MCP server over stdio (ARCHITECTURE.md §10). Read-only Actions and
//! read services are tools; mutating Actions are exposed only when the
//! server is started with mutations allowed, and even then every mutation
//! returns a pending confirmation that a second `confirm_action` call must
//! approve. An external agent is a client like the UI and the Director,
//! never a back door: it goes through the same Runtime and the same policy.
//!
//! The protocol subset implemented: `initialize`, `notifications/initialized`,
//! `ping`, `tools/list`, `tools/call`. JSON-RPC 2.0, one message per line.

use riddle_app::App;
use riddle_core::*;
use riddle_errors::{ErrorKind, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub struct McpServer {
    app: Arc<App>,
    allow_mutations: bool,
}

impl McpServer {
    pub fn new(app: Arc<App>, allow_mutations: bool) -> Self {
        Self {
            app,
            allow_mutations,
        }
    }

    /// The tool catalogue: read services, then Actions filtered by policy.
    pub fn tools(&self) -> Vec<Value> {
        let mut tools = vec![
            tool("summary", "The morning summary: counts of Finds, Unresolved, Filed, Spoil and Contamination, and whether Riddle is caught up.", json!({"type": "object", "properties": {}})),
            tool("list_digs", "Connected accounts (Digs) with status.", json!({"type": "object", "properties": {}})),
            tool("list_messages", "Page through messages, newest first, optionally filtered.", json!({"type": "object", "properties": {"digId": {"type": "string"}, "disposition": {"type": "string", "enum": ["find", "filed", "spoil", "contamination", "unresolved"]}, "query": {"type": "string"}, "limit": {"type": "integer"}, "cursor": {"type": "string"}}})),
            tool("get_message", "One message with its body.", json!({"type": "object", "required": ["digId", "messageId"], "properties": {"digId": {"type": "string"}, "messageId": {"type": "string"}}})),
            tool("explain", "Why a message has its disposition: the Mesh, the Signals, the gate, the escalation.", json!({"type": "object", "required": ["digId", "messageId"], "properties": {"digId": {"type": "string"}, "messageId": {"type": "string"}}})),
            tool("list_finds", "Finds needing attention.", json!({"type": "object", "properties": {"status": {"type": "string", "enum": ["open", "snoozed", "resolved", "dismissed"]}, "limit": {"type": "integer"}}})),
            tool("list_activity", "The Activity ledger, newest first.", json!({"type": "object", "properties": {"limit": {"type": "integer"}}})),
            tool("list_meshes", "Meshes (policy) at their latest version.", json!({"type": "object", "properties": {}})),
            tool("list_signals", "The Signal Registry.", json!({"type": "object", "properties": {}})),
        ];
        for meta in self.app.runtime.list_actions() {
            if meta.read_only || self.allow_mutations {
                let mut desc = meta.description.clone();
                if !meta.read_only {
                    desc.push_str(&format!(" [mutating; risk {:?}; returns a pending confirmation — call confirm_action with its id]", meta.risk));
                }
                tools.push(tool(&format!("action.{}", meta.name), &desc, json!({"type": "object", "properties": {"args": {"type": "object"}, "digId": {"type": "string"}}})));
            }
        }
        if self.allow_mutations {
            tools.push(tool("confirm_action", "Approve a pending action returned by a mutating tool.", json!({"type": "object", "required": ["pendingId"], "properties": {"pendingId": {"type": "string"}}})));
        }
        tools
    }

    pub async fn call(&self, name: &str, args: Value) -> Result<Value, RiddleError> {
        let app = &self.app;
        let str_arg = |k: &str| args.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
        let limit = args.get("limit").and_then(|v| v.as_u64()).map(|n| n as u32);
        Ok(match name {
            "summary" => serde_json::to_value(
                app.summary(chrono::Utc::now() - chrono::Duration::hours(24))?,
            )?,
            "list_digs" => serde_json::to_value(app.list_digs()?)?,
            "list_messages" => {
                let filter = riddle_store::repo::messages::MessageFilter {
                    disposition: str_arg("disposition"),
                    query: str_arg("query"),
                    ..Default::default()
                };
                let page = PageRequest {
                    cursor: str_arg("cursor"),
                    limit,
                };
                serde_json::to_value(app.list_messages(
                    str_arg("digId").map(DigId::from).as_ref(),
                    &filter,
                    &page,
                )?)?
            }
            "get_message" => {
                let dig = DigId::from(
                    str_arg("digId").ok_or_else(|| RiddleError::invalid("digId required"))?,
                );
                let id = MessageId::from(
                    str_arg("messageId")
                        .ok_or_else(|| RiddleError::invalid("messageId required"))?,
                );
                serde_json::to_value(app.get_message(&dig, &id)?)?
            }
            "explain" => {
                let dig = DigId::from(
                    str_arg("digId").ok_or_else(|| RiddleError::invalid("digId required"))?,
                );
                let id = MessageId::from(
                    str_arg("messageId")
                        .ok_or_else(|| RiddleError::invalid("messageId required"))?,
                );
                serde_json::to_value(app.explain(&dig, &id)?)?
            }
            "list_finds" => {
                let status = str_arg("status")
                    .and_then(|s| match s.as_str() {
                        "open" => Some(FindStatus::Open),
                        "snoozed" => Some(FindStatus::Snoozed),
                        "resolved" => Some(FindStatus::Resolved),
                        "dismissed" => Some(FindStatus::Dismissed),
                        _ => None,
                    })
                    .or(Some(FindStatus::Open));
                serde_json::to_value(app.list_finds(
                    &riddle_store::repo::finds::FindFilter {
                        status,
                        ..Default::default()
                    },
                    &PageRequest {
                        cursor: None,
                        limit,
                    },
                )?)?
            }
            "list_activity" => serde_json::to_value(app.list_activity(
                &Default::default(),
                &PageRequest {
                    cursor: None,
                    limit,
                },
            )?)?,
            "list_meshes" => serde_json::to_value(riddle_store::repo::meshes::list_latest(
                &app.stores.index(),
                false,
            )?)?,
            "list_signals" => {
                serde_json::to_value(riddle_store::repo::signals::list(&app.stores.index())?)?
            }
            "confirm_action" => {
                if !self.allow_mutations {
                    return Err(RiddleError::policy_denied(
                        "mutations are not enabled for this server",
                    ));
                }
                let pid = str_arg("pendingId")
                    .ok_or_else(|| RiddleError::invalid("pendingId required"))?;
                let pending = riddle_store::repo::pending::get_pending(&app.stores.index(), &pid)?
                    .ok_or_else(|| RiddleError::not_found("pending action"))?;
                let inv = ActionInvocation {
                    action: pending.action.clone(),
                    args: pending.args.clone(),
                    principal: Principal::Mcp,
                    dig_id: None,
                    idempotency_key: None,
                    confirmed: false,
                    pending_id: Some(pid),
                };
                serde_json::to_value(app.runtime.invoke(inv).await?)?
            }
            n if n.starts_with("action.") => {
                let action = &n["action.".len()..];
                let meta = app.runtime.action(action)?.meta();
                if !meta.read_only && !self.allow_mutations {
                    return Err(RiddleError::policy_denied(format!(
                        "{action} mutates state; start the server with mutations allowed"
                    )));
                }
                let inv = ActionInvocation {
                    action: action.to_string(),
                    args: args.get("args").cloned().unwrap_or(json!({})),
                    principal: Principal::Mcp,
                    dig_id: str_arg("digId").map(DigId::from),
                    idempotency_key: None,
                    confirmed: false,
                    pending_id: None,
                };
                match app.runtime.invoke(inv).await {
                    Ok(out) => serde_json::to_value(out)?,
                    Err(e) if e.kind == ErrorKind::NeedsConfirmation => {
                        json!({ "pending": e.detail, "message": "confirm with confirm_action" })
                    }
                    Err(e) => return Err(e),
                }
            }
            _ => return Err(RiddleError::not_found(format!("no tool named {name}"))),
        })
    }

    /// Handle one JSON-RPC request; `None` for notifications.
    pub async fn handle(&self, msg: Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "riddle", "version": env!("CARGO_PKG_VERSION") }
            })),
            "notifications/initialized" | "notifications/cancelled" => return None,
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": self.tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.call(name, args).await {
                    Ok(v) => Ok(
                        json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }], "isError": false }),
                    ),
                    Err(e) => Ok(
                        json!({ "content": [{ "type": "text", "text": format!("{}: {}", e.kind.as_str(), e.message) }], "isError": true }),
                    ),
                }
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        let id = id?;
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        })
    }

    /// Serve on stdin/stdout until EOF.
    pub async fn serve_stdio(&self) -> std::io::Result<()> {
        rinfo!(
            Category::Actions,
            "mcp server started; mutations={}",
            self.allow_mutations
        );
        let stdin = BufReader::new(tokio::io::stdin());
        let mut stdout = tokio::io::stdout();
        let mut lines = stdin.lines();
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            let msg: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    rwarn!(Category::Actions, "mcp: bad json: {e}");
                    let err = json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "parse error" } });
                    stdout.write_all(format!("{err}\n").as_bytes()).await?;
                    continue;
                }
            };
            if let Some(resp) = self.handle(msg).await {
                stdout.write_all(format!("{resp}\n").as_bytes()).await?;
                stdout.flush().await?;
            }
        }
        Ok(())
    }
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_app::AppConfig;

    fn server(allow: bool) -> McpServer {
        let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
            "0", None, None, None, "test",
        )))
        .unwrap();
        McpServer::new(app, allow)
    }

    #[tokio::test]
    async fn initialize_and_list_tools_answer_over_json_rpc() {
        let s = server(false);
        let init = s
            .handle(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
            .await
            .unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "riddle");
        let list = s
            .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
            .await
            .unwrap();
        let names: Vec<&str> = list["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"summary"));
        assert!(!names.contains(&"confirm_action"));
    }

    #[tokio::test]
    async fn a_notification_gets_no_response_and_an_unknown_method_an_error() {
        let s = server(false);
        assert!(s
            .handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await
            .is_none());
        let r = s
            .handle(json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}))
            .await
            .unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn calling_summary_returns_text_content() {
        let s = server(false);
        let r = s.handle(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "summary", "arguments": {}}})).await.unwrap();
        assert_eq!(r["result"]["isError"], false);
        assert!(r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("caughtUp"));
    }
}
