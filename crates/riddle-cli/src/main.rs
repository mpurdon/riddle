//! `riddle`: the CLI twin of the desktop app (ARCHITECTURE.md §10). It opens
//! the same stores and the same Action runtime, so anything the UI can do a
//! script can do. stderr is for humans; stdout is for machines. `--progress`
//! emits NDJSON, prints the plan before the first side effect, and implies
//! `--yes`.

use clap::{Parser, Subcommand};
use riddle_actions::Runtime;
use riddle_app::paths::{AppDirs, Profile};
use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_errors::{ErrorKind, RiddleError};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "riddle", version, about = "Riddle from the command line")]
struct Cli {
    /// Emit NDJSON progress on stdout (implies --yes).
    #[arg(long, global = true)]
    progress: bool,
    /// Confirm mutating actions without prompting.
    #[arg(long, global = true)]
    yes: bool,
    /// Use the production data directory even in a debug build.
    #[arg(long, global = true)]
    prod: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the morning summary.
    Summary,
    /// List Digs and their status.
    Digs,
    /// Actions: list, or invoke one by name with JSON args.
    Actions {
        #[command(subcommand)]
        sub: ActionsCmd,
    },
    /// Invoke an Action (shorthand for `actions invoke`).
    Invoke {
        name: String,
        /// JSON object of arguments.
        #[arg(long, default_value = "{}")]
        args: String,
        #[arg(long)]
        dig: Option<String>,
    },
    /// List Finds.
    Finds {
        #[arg(long, default_value = "open")]
        status: String,
    },
    /// List messages in a Dig.
    Messages {
        #[arg(long)]
        dig: String,
        #[arg(long)]
        disposition: Option<String>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Explain why a message has its disposition.
    Explain {
        #[arg(long)]
        dig: String,
        message: String,
    },
    /// Sync a Dig now (enqueues refresh through the Action layer).
    Sync {
        #[arg(long)]
        dig: Option<String>,
    },
    /// Simulate a Mesh file against history.
    Simulate {
        file: std::path::PathBuf,
        #[arg(long, default_value_t = 90)]
        window_days: u32,
    },
    /// Export decision traces as JSON (redacted by default).
    Export {
        #[command(subcommand)]
        sub: ExportCmd,
    },
    /// Show the Activity ledger.
    Activity {
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Show where Riddle keeps its files.
    Paths,
    /// Run the loopback dev bridge (HTTP + SSE) for automation and browser UI development.
    Bridge {
        #[arg(long, default_value_t = 1431)]
        port: u16,
    },
    /// Run Riddle headless: jobs, sync, Sift, projection, and the bridge, until Ctrl-C.
    Serve {
        #[arg(long, default_value_t = 1431)]
        port: u16,
        /// Use the scripted judge and no LLM (no network, no spend).
        #[arg(long)]
        offline: bool,
    },
    /// Run one engine pass over every Dig (sync, sift, project, temporal) and print the report.
    Pass {
        #[arg(long, default_value_t = 200)]
        sift_limit: u32,
        #[arg(long)]
        offline: bool,
    },
    /// Serve the MCP protocol on stdin/stdout for external agents.
    Mcp {
        /// Expose mutating Actions (each still returns a pending confirmation).
        #[arg(long)]
        allow_mutations: bool,
    },
}

#[derive(Subcommand)]
enum ActionsCmd {
    List,
    Invoke {
        name: String,
        #[arg(long, default_value = "{}")]
        args: String,
        #[arg(long)]
        dig: Option<String>,
    },
}

#[derive(Subcommand)]
enum ExportCmd {
    Traces {
        #[arg(long)]
        dig: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: u32,
        /// Include message identifiers only (default). `--full` keeps facts.
        #[arg(long)]
        full: bool,
    },
}

fn progress_line(v: serde_json::Value) {
    println!("{}", v);
}

async fn open_app(prod: bool) -> Result<Arc<App>, RiddleError> {
    let profile = if prod { Profile::Prod } else { Profile::current() };
    let dirs = AppDirs::resolve(profile);
    let build = BuildStamp::from_env_vars(env!("CARGO_PKG_VERSION"), option_env!("RIDDLE_GIT_SHA"), option_env!("RIDDLE_GIT_DIRTY"), option_env!("RIDDLE_BUILT_AT"), profile.as_str());
    App::open(AppConfig { data_dir: Some(dirs.data), config_dir: Some(dirs.config), dev: profile.is_dev(), build, keychain_service: profile.keychain_service() })
}

async fn invoke(rt: &Runtime, name: &str, args: &str, dig: Option<String>, yes: bool, progress: bool) -> Result<serde_json::Value, RiddleError> {
    let args: serde_json::Value = serde_json::from_str(args).map_err(|e| RiddleError::invalid(format!("--args must be a JSON object: {e}")))?;
    let mut inv = ActionInvocation { action: name.to_string(), args, principal: Principal::Cli, dig_id: dig.map(DigId::from), idempotency_key: None, confirmed: yes, pending_id: None };
    loop {
        match rt.invoke(inv.clone()).await {
            Ok(out) => return Ok(serde_json::json!({ "result": out.result, "activityId": out.activity_id, "replayed": out.replayed })),
            Err(e) if e.kind == ErrorKind::NeedsConfirmation => {
                let pending: PendingAction = e.detail.clone().and_then(|d| serde_json::from_value(d).ok()).ok_or(e)?;
                if progress {
                    progress_line(serde_json::json!({ "type": "plan", "action": pending.action, "summary": pending.summary, "realArguments": pending.real_arguments }));
                    inv.pending_id = Some(pending.id);
                    continue;
                }
                eprintln!("{}\n{}", pending.summary, serde_json::to_string_pretty(&pending.real_arguments).unwrap_or_default());
                eprint!("Proceed? [y/N] ");
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).ok();
                if line.trim().eq_ignore_ascii_case("y") {
                    inv.pending_id = Some(pending.id);
                    continue;
                }
                return Err(RiddleError::policy_denied("not confirmed"));
            }
            Err(e) => return Err(e),
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).target(env_logger::Target::Stderr).init();
    let cli = Cli::parse();
    let yes = cli.yes || cli.progress;
    let code = match run(cli, yes).await {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {} ({})", e.message, e.kind.as_str());
            match e.kind {
                ErrorKind::NotFound => 4,
                ErrorKind::Invalid => 2,
                ErrorKind::PolicyDenied | ErrorKind::NeedsConfirmation => 3,
                _ => 1,
            }
        }
    };
    std::process::exit(code);
}

async fn run(cli: Cli, yes: bool) -> Result<(), RiddleError> {
    if let Command::Paths = cli.command {
        let profile = if cli.prod { Profile::Prod } else { Profile::current() };
        let dirs = AppDirs::resolve(profile);
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "profile": profile.as_str(), "data": dirs.data, "config": dirs.config, "logs": dirs.logs, "cache": dirs.cache }))?);
        return Ok(());
    }
    let app = open_app(cli.prod).await?;
    let offline = matches!(cli.command, Command::Serve { offline: true, .. } | Command::Pass { offline: true, .. });
    let background = matches!(cli.command, Command::Serve { .. });
    let overrides = if offline {
        let mut s = riddle_settings::Settings::dev_defaults();
        s.judge.provider = riddle_settings::JudgeProvider::Scripted;
        s.llm.provider = riddle_settings::LlmProvider::Null;
        Some(s)
    } else {
        None
    };
    let ops = app.boot_with(background, overrides).await?;
    if cli.progress {
        let mut rx = app.events.subscribe();
        tokio::spawn(async move {
            while let Ok(e) = rx.recv().await {
                progress_line(serde_json::json!({ "type": "event", "event": e }));
            }
        });
    }
    let out: serde_json::Value = match cli.command {
        Command::Summary => serde_json::to_value(app.summary(chrono::Utc::now() - chrono::Duration::hours(24))?)?,
        Command::Digs => serde_json::to_value(app.list_digs()?)?,
        Command::Actions { sub } => match sub {
            ActionsCmd::List => serde_json::to_value(app.runtime.list_actions())?,
            ActionsCmd::Invoke { name, args, dig } => invoke(&app.runtime, &name, &args, dig, yes, cli.progress).await?,
        },
        Command::Invoke { name, args, dig } => invoke(&app.runtime, &name, &args, dig, yes, cli.progress).await?,
        Command::Finds { status } => {
            let status = match status.as_str() { "open" => Some(FindStatus::Open), "snoozed" => Some(FindStatus::Snoozed), "resolved" => Some(FindStatus::Resolved), "dismissed" => Some(FindStatus::Dismissed), "all" => None, _ => return Err(RiddleError::invalid("status must be open|snoozed|resolved|dismissed|all")) };
            serde_json::to_value(app.list_finds(&riddle_store::repo::finds::FindFilter { status, ..Default::default() }, &PageRequest { cursor: None, limit: Some(200) })?)?
        }
        Command::Messages { dig, disposition, query, limit } => {
            let filter = riddle_store::repo::messages::MessageFilter { disposition, query, ..Default::default() };
            serde_json::to_value(app.list_messages(Some(&DigId::from(dig)), &filter, &PageRequest { cursor: None, limit: Some(limit) })?)?
        }
        Command::Explain { dig, message } => serde_json::to_value(app.explain(&DigId::from(dig), &MessageId::from(message))?)?,
        Command::Sync { dig } => invoke(&app.runtime, "refresh_now", &serde_json::json!({ "digId": dig }).to_string(), None, true, cli.progress).await?,
        Command::Simulate { file, window_days } => {
            let text = std::fs::read_to_string(&file)?;
            invoke(&app.runtime, "simulate_mesh", &serde_json::json!({ "text": text, "windowDays": window_days }).to_string(), None, true, cli.progress).await?
        }
        Command::Export { sub } => match sub {
            ExportCmd::Traces { dig, limit, full } => {
                let traces = app.recent_traces(dig.map(DigId::from).as_ref(), limit)?;
                let redacted: Vec<serde_json::Value> = traces
                    .into_iter()
                    .map(|t| {
                        let mut v = serde_json::to_value(&t).unwrap_or_default();
                        if !full {
                            if let Some(obj) = v.as_object_mut() {
                                obj.remove("facts");
                            }
                        }
                        v
                    })
                    .collect();
                serde_json::Value::Array(redacted)
            }
        },
        Command::Activity { limit } => serde_json::to_value(app.list_activity(&Default::default(), &PageRequest { cursor: None, limit: Some(limit) })?)?,
        Command::Serve { port, .. } => {
            let profile = if cli.prod { Profile::Prod } else { Profile::current() };
            let dirs = AppDirs::resolve(profile);
            let info = riddle_app::bridge::start(app.clone(), port, Some(dirs.data.join("bridge.json"))).await?;
            eprintln!("riddle serving; bridge on http://127.0.0.1:{} ({} actions)", info.port, app.runtime.list_actions().len());
            println!("{}", serde_json::json!({ "port": info.port, "token": info.token }));
            tokio::signal::ctrl_c().await?;
            ops.runner.stop();
            return Ok(());
        }
        Command::Pass { sift_limit, .. } => {
            let reports = app.full_pass_all(sift_limit, &riddle_sync::engine::CancelFlag::new()).await?;
            for d in app.list_digs()? { app.refresh_dig_status(&d.id); }
            serde_json::to_value(reports)?
        }
        Command::Bridge { port } => {
            let profile = if cli.prod { Profile::Prod } else { Profile::current() };
            let dirs = AppDirs::resolve(profile);
            let info = riddle_app::bridge::start(app.clone(), port, Some(dirs.data.join("bridge.json"))).await?;
            eprintln!("bridge on http://127.0.0.1:{} (token in {})", info.port, dirs.data.join("bridge.json").display());
            println!("{}", serde_json::json!({ "port": info.port, "token": info.token }));
            tokio::signal::ctrl_c().await?;
            return Ok(());
        }
        Command::Mcp { allow_mutations } => {
            riddle_mcp::McpServer::new(app.clone(), allow_mutations).serve_stdio().await?;
            return Ok(());
        }
        Command::Paths => unreachable!(),
    };
    if cli.progress {
        progress_line(serde_json::json!({ "type": "result", "result": out }));
    } else {
        println!("{}", serde_json::to_string_pretty(&out)?);
    }
    Ok(())
}
