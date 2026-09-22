//! The loopback dev bridge: the command router over HTTP on 127.0.0.1 plus
//! an SSE stream of events. It exists so automation (tests, agents, a
//! browser tab running the React client) can drive the real core without
//! the Tauri window. Dev builds only, loopback only, bearer token written
//! 0600 into the data directory; a production build never starts it
//! (ARCHITECTURE.md §59, §91: more visibility, never more authority — every
//! call still goes through the Action runtime and its policy).
//!
//! Hand-rolled HTTP/1.1 on tokio rather than a framework: two routes, small
//! bodies, and one less heavyweight dependency in the core.

use crate::{dispatch, App};
use riddle_observability::{rinfo, rwarn, Category};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

pub struct BridgeInfo {
    pub port: u16,
    pub token: String,
}

/// Start the bridge and return its port and token. `port` 0 picks a free one.
pub async fn start(app: Arc<App>, port: u16, token_file: Option<std::path::PathBuf>) -> std::io::Result<BridgeInfo> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let port = listener.local_addr()?.port();
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    if let Some(path) = &token_file {
        write_private(path, &format!("{{\"port\":{port},\"token\":\"{token}\"}}\n"))?;
    }
    rinfo!(Category::App, "dev bridge listening on 127.0.0.1:{port}");
    let tok = token.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let app = app.clone();
                    let tok = tok.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle(stream, app, tok).await {
                            rwarn!(Category::App, "bridge connection error: {e}");
                        }
                    });
                }
                Err(e) => {
                    rwarn!(Category::App, "bridge accept failed: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    });
    Ok(BridgeInfo { port, token })
}

fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(contents.as_bytes())?;
    }
    std::fs::rename(tmp, path)
}

const CORS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\n";

async fn handle(stream: TcpStream, app: Arc<App>, token: String) -> std::io::Result<()> {
    let (rd, mut wr) = stream.into_split();
    let mut reader = BufReader::new(rd);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let mut content_length = 0usize;
    let mut auth = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            match k.to_ascii_lowercase().as_str() {
                "content-length" => content_length = v.trim().parse().unwrap_or(0),
                "authorization" => auth = v.trim().to_string(),
                _ => {}
            }
        }
    }
    if method == "OPTIONS" {
        wr.write_all(format!("HTTP/1.1 204 No Content\r\n{CORS}Content-Length: 0\r\n\r\n").as_bytes()).await?;
        return Ok(());
    }
    let path_only = path.split('?').next().unwrap_or("/");
    let query_token = path.split('?').nth(1).and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=").map(|s| s.to_string())));
    let presented = auth.strip_prefix("Bearer ").map(|s| s.to_string()).or(query_token);
    if presented.as_deref() != Some(token.as_str()) {
        wr.write_all(format!("HTTP/1.1 401 Unauthorized\r\n{CORS}Content-Length: 0\r\n\r\n").as_bytes()).await?;
        return Ok(());
    }
    match (method.as_str(), path_only) {
        ("POST", "/invoke") => {
            let mut body = vec![0u8; content_length.min(16 * 1024 * 1024)];
            reader.read_exact(&mut body).await?;
            let req: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
            let command = req.get("command").and_then(|c| c.as_str()).unwrap_or("").to_string();
            let args = req.get("args").cloned().unwrap_or(json!({}));
            let (status, payload) = match dispatch::call(&app, &command, args).await {
                Ok(v) => ("200 OK", json!({ "ok": true, "result": v })),
                Err(e) => ("200 OK", json!({ "ok": false, "error": e })),
            };
            let bytes = serde_json::to_vec(&payload).unwrap_or_default();
            wr.write_all(format!("HTTP/1.1 {status}\r\n{CORS}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n", bytes.len()).as_bytes()).await?;
            wr.write_all(&bytes).await?;
        }
        ("GET", "/events") => {
            wr.write_all(format!("HTTP/1.1 200 OK\r\n{CORS}Content-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n").as_bytes()).await?;
            wr.write_all(b": connected\n\n").await?;
            let mut rx = app.events.subscribe();
            loop {
                tokio::select! {
                    ev = rx.recv() => match ev {
                        Ok(e) => {
                            let line = format!("data: {}\n\n", serde_json::to_string(&e).unwrap_or_default());
                            if wr.write_all(line.as_bytes()).await.is_err() { break; }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    },
                    _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                        if wr.write_all(b": keepalive\n\n").await.is_err() { break; }
                    }
                }
            }
        }
        ("GET", "/commands") => {
            let bytes = serde_json::to_vec(&dispatch::COMMANDS).unwrap_or_default();
            wr.write_all(format!("HTTP/1.1 200 OK\r\n{CORS}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n", bytes.len()).as_bytes()).await?;
            wr.write_all(&bytes).await?;
        }
        _ => {
            wr.write_all(format!("HTTP/1.1 404 Not Found\r\n{CORS}Content-Length: 0\r\n\r\n").as_bytes()).await?;
        }
    }
    wr.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppConfig;
    use riddle_core::BuildStamp;
    use tokio::io::AsyncWriteExt;

    async fn post(port: u16, token: &str, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("POST /invoke HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }

    #[tokio::test]
    async fn the_bridge_answers_a_command_and_rejects_a_bad_token() {
        let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars("0", None, None, None, "test"))).unwrap();
        let info = start(app, 0, None).await.unwrap();
        let ok = post(info.port, &info.token, r#"{"command":"summary","args":{}}"#).await;
        assert!(ok.contains("200 OK") && ok.contains("caughtUp"));
        let bad = post(info.port, "nope", r#"{"command":"summary","args":{}}"#).await;
        assert!(bad.contains("401"));
        let unknown = post(info.port, &info.token, r#"{"command":"nope","args":{}}"#).await;
        assert!(unknown.contains("not_found"));
    }
}
