//! The command router: one function from (command name, JSON args) to a JSON
//! result, covering the read surface in `docs/COMMANDS.md` plus
//! `invoke_action`. The Tauri commands are typed wrappers over the same
//! read services; the loopback bridge and any automation call this router
//! directly, so behaviour is identical whichever door a client uses.

use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

fn arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    let v = args.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| RiddleError::invalid(format!("argument {key}: {e}")))
}

fn opt<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Option<T> {
    args.get(key)
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
}

/// A message by `digId` and `messageId`, or by a link copied from its page
/// (`riddle://message/<digId>/<messageId> …`) passed whole as `messageId`,
/// which carries both.
fn message_args(args: &Value) -> Result<(DigId, MessageId)> {
    if let Some(pair) = opt::<String>(args, "messageId").and_then(|s| parse_message_link(&s)) {
        return Ok(pair);
    }
    Ok((arg(args, "digId")?, arg(args, "messageId")?))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageFilterArgs {
    pub disposition: Option<String>,
    pub find_type: Option<String>,
    pub label: Option<String>,
    pub query: Option<String>,
    pub from: Option<String>,
    pub thread_id: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub stratum: Option<String>,
    #[serde(default)]
    pub unclassified_only: bool,
    /// The Shadow review list: only messages with a proposal, shown as the proposal.
    #[serde(default)]
    pub shadow_only: bool,
    /// `false` hides mail a newer one of its series superseded, `true`
    /// lists only that; absent is both.
    pub superseded: Option<bool>,
    /// Only messages at this level or above.
    #[serde(default)]
    pub min_importance: Option<riddle_core::Importance>,
    /// A place in the mail column (`label:Receipts`, `topic:Receipts`),
    /// resolved per account by `App::list_messages_at`.
    #[serde(default)]
    pub place: Option<Place>,
}

impl From<MessageFilterArgs> for repo::messages::MessageFilter {
    fn from(a: MessageFilterArgs) -> Self {
        Self {
            disposition: a.disposition,
            find_type: a.find_type,
            label: a.label,
            query: a.query,
            from: a.from,
            thread_id: a.thread_id,
            since: a.since,
            until: a.until,
            unclassified_only: a.unclassified_only,
            include_shadow: false,
            shadow_only: a.shadow_only,
            stratum: a.stratum,
            priority: None,
            superseded: a.superseded,
            min_importance: a.min_importance,
            any_of: None,
        }
    }
}

/// Names the router answers, for `tools/list`-style introspection.
pub const COMMANDS: &[&str] = &[
    "summary",
    "list_digs",
    "list_dig_groups",
    "list_messages",
    "get_message",
    "get_thread",
    "explain",
    "search",
    "priority_inbox",
    "list_finds",
    "get_find",
    "list_activity",
    "get_activity",
    "list_meshes",
    "get_mesh",
    "mesh_versions",
    "list_signals",
    "signal_versions",
    "signal_evaluations",
    "list_strata",
    "list_expeditions",
    "get_expedition",
    "list_drafts",
    "get_user_profile",
    "read_trace",
    "recent_traces",
    "usage_summary",
    "trace_stats",
    "trust_stats",
    "invoke_action",
    "list_actions",
    "list_pending",
    "dev_armed",
    "mailbox_writes",
    "comparison_overview",
    "comparison_rows",
    "director_comparisons",
    "list_labels",
    "mail_places",
    "list_conversations",
    "director_audit",
    "list_proposals",
    "get_conversation",
    "director_send",
    "director_retry",
    "task_centre",
    "get_settings",
    "privacy_statement",
    "probe_judge",
    "probe_llm",
    "probe_provider",
    "update_preview",
    "parse_mesh",
    "simulate_mesh",
    "mesh_order",
    "list_named_lists",
    "list_packs",
    "list_suggestions",
    "rule_check",
    "rule_samples",
    "ui_log",
    "dev_info",
    "read_app_logs",
    "log_categories",
    "export_diagnostics",
    "clear_caches",
];

pub async fn call(app: &Arc<App>, command: &str, args: Value) -> Result<Value> {
    let index = app.stores.index();
    Ok(match command {
        "summary" => serde_json::to_value(app.summary(
            chrono::Utc::now() - chrono::Duration::hours(24),
            opt::<DigId>(&args, "digId").as_ref(),
        )?)?,
        // The client manages accounts, so it sees disabled ones too and
        // leaves them out of its views itself.
        "list_digs" => serde_json::to_value(app.list_all_digs()?)?,
        "list_dig_groups" => serde_json::to_value(app.list_dig_groups()?)?,
        "list_messages" => {
            let mut filter: MessageFilterArgs = match args.get("filter") {
                Some(f) if !f.is_null() => serde_json::from_value(f.clone())?,
                _ => MessageFilterArgs::default(),
            };
            let place = filter.place.take();
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.list_messages_at(
                opt::<DigId>(&args, "digId").as_ref(),
                &filter.into(),
                place.as_ref(),
                &page,
            )?)?
        }
        "get_message" => {
            let (dig, id) = message_args(&args)?;
            serde_json::to_value(app.get_message(&dig, &id)?)?
        }
        "get_thread" => {
            serde_json::to_value(app.get_thread(&arg(&args, "digId")?, &arg(&args, "threadId")?)?)?
        }
        "explain" => {
            let (dig, id) = message_args(&args)?;
            serde_json::to_value(app.explain(&dig, &id)?)?
        }
        "search" => {
            let filter = repo::messages::MessageFilter {
                query: Some(arg(&args, "query")?),
                shadow_only: opt::<bool>(&args, "shadowOnly").unwrap_or(false),
                ..Default::default()
            };
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.list_messages(
                opt::<DigId>(&args, "digId").as_ref(),
                &filter,
                &page,
            )?)?
        }
        "priority_inbox" => {
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.priority_inbox(opt::<DigId>(&args, "digId").as_ref(), &page)?)?
        }
        "list_finds" => {
            let status = opt::<String>(&args, "status").and_then(|s| match s.as_str() {
                "open" => Some(FindStatus::Open),
                "snoozed" => Some(FindStatus::Snoozed),
                "resolved" => Some(FindStatus::Resolved),
                "dismissed" => Some(FindStatus::Dismissed),
                "expired" => Some(FindStatus::Expired),
                "superseded" => Some(FindStatus::Superseded),
                _ => None,
            });
            let find_type = opt::<String>(&args, "findType").and_then(|s| FindType::parse(&s));
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.list_finds(
                &repo::finds::FindFilter {
                    status,
                    find_type,
                    dig_id: opt(&args, "digId"),
                    min_priority: None,
                },
                &page,
            )?)?
        }
        "get_find" => serde_json::to_value(
            repo::finds::get(&index, &arg(&args, "findId")?)?
                .ok_or_else(|| RiddleError::not_found("find"))?,
        )?,
        "list_activity" => {
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            let filter = repo::activities::ActivityFilter {
                dig_id: opt(&args, "digId"),
                actor: opt(&args, "actor"),
                action: opt(&args, "action"),
                since: opt(&args, "since"),
                until: opt(&args, "until"),
                conversation_id: opt(&args, "conversationId"),
            };
            serde_json::to_value(app.list_activity(&filter, &page)?)?
        }
        "get_activity" => serde_json::to_value(
            repo::activities::get(&index, &arg(&args, "activityId")?)?
                .ok_or_else(|| RiddleError::not_found("activity"))?,
        )?,
        // Mesh reads go through the service, which gives a Mesh stored before
        // routes carried their wording its sentences on the way out.
        "list_meshes" => serde_json::to_value(
            riddle_mesh::service::MeshService::new(index.clone())
                .list(opt(&args, "includeDeleted").unwrap_or(false))?,
        )?,
        "get_mesh" => {
            let id: MeshId = arg(&args, "meshId")?;
            let svc = riddle_mesh::service::MeshService::new(index.clone());
            serde_json::to_value(match opt::<u32>(&args, "version") {
                Some(v) => svc.get(&id, v)?,
                None => svc.latest(&id)?,
            })?
        }
        "mesh_versions" => serde_json::to_value(
            riddle_mesh::service::MeshService::new(index.clone())
                .versions(&arg(&args, "meshId")?)?,
        )?,
        "list_signals" => serde_json::to_value(repo::signals::list(&index)?)?,
        "signal_versions" => serde_json::to_value(repo::signals::versions(
            &index,
            &arg::<String>(&args, "name")?,
        )?)?,
        "signal_evaluations" => serde_json::to_value(
            app.signal_evaluations(&arg(&args, "digId")?, &arg(&args, "messageId")?)?,
        )?,
        "list_strata" => serde_json::to_value(repo::strata::list(&index, false)?)?,
        "list_expeditions" => serde_json::to_value(repo::expeditions::list(&index, 100)?)?,
        "get_expedition" => serde_json::to_value(
            repo::expeditions::get(&index, &arg(&args, "expeditionId")?)?
                .ok_or_else(|| RiddleError::not_found("expedition"))?,
        )?,
        "list_drafts" => {
            let store = app.stores.dig(&arg(&args, "digId")?)?;
            serde_json::to_value(repo::drafts::list(&store)?)?
        }
        "get_user_profile" => serde_json::to_value(repo::user_profile::get(&index)?)?,
        "read_trace" => {
            serde_json::to_value(app.read_trace(&arg(&args, "digId")?, &arg(&args, "traceId")?)?)?
        }
        "recent_traces" => serde_json::to_value(app.recent_traces(
            opt::<DigId>(&args, "digId").as_ref(),
            opt(&args, "limit").unwrap_or(50),
        )?)?,
        "usage_summary" => serde_json::to_value(app.usage_summary(
            chrono::Utc::now()
                - chrono::Duration::days(opt::<i64>(&args, "sinceDays").unwrap_or(30)),
        )?)?,
        "trust_stats" => serde_json::to_value(app.trust_stats(
            opt::<u32>(&args, "sinceDays").unwrap_or(7),
            opt::<DigId>(&args, "digId").as_ref(),
        )?)?,
        "trace_stats" => {
            let since = chrono::Utc::now()
                - chrono::Duration::days(opt::<i64>(&args, "sinceDays").unwrap_or(30));
            let digs = match opt::<DigId>(&args, "digId") {
                Some(d) => vec![app.dig(&d)?],
                None => app.list_digs()?,
            };
            serde_json::to_value(trace_stats(app, &digs, &since)?)?
        }
        "invoke_action" => {
            let inv: ActionInvocation = arg(&args, "invocation")?;
            serde_json::to_value(app.runtime.invoke(inv).await?)?
        }
        "list_actions" => serde_json::to_value(app.runtime.list_actions())?,
        "list_pending" => serde_json::to_value(repo::pending::list_pending(&index)?)?,
        "dev_armed" => json!(app.runtime.is_armed()),
        "mailbox_writes" => serde_json::to_value(app.mailbox_writes()?)?,
        "comparison_overview" => serde_json::to_value(app.comparison_overview()?)?,
        "comparison_rows" => serde_json::to_value(
            app.comparison_rows(args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize)?,
        )?,
        "director_comparisons" => serde_json::to_value(app.director_comparisons(
            args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize,
        )?)?,
        "mail_places" => {
            serde_json::to_value(app.mail_places(opt::<DigId>(&args, "digId").as_ref())?)?
        }
        "list_labels" => {
            let dig: Option<DigId> = args
                .get("digId")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?;
            serde_json::to_value(app.list_labels(dig.as_ref())?)?
        }
        "list_conversations" => serde_json::to_value(repo::conversations::list(&index, 100)?)?,
        // What Riddle noticed about its own rules: senders whose open Finds
        // pile up on one subject. Read by the Director page's empty state.
        "list_suggestions" => serde_json::to_value(app.suggestions(&[])?)?,
        "director_audit" => {
            serde_json::to_value(app.director_audit(&arg::<String>(&args, "conversationId")?)?)?
        }
        "list_proposals" => serde_json::to_value(repo::pending::proposals(
            &index,
            args.get("conversationId").and_then(|v| v.as_str()),
            args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as u32,
        )?)?,
        "get_conversation" => serde_json::to_value(
            repo::conversations::get(&index, &arg(&args, "conversationId")?)?
                .ok_or_else(|| RiddleError::not_found("conversation"))?,
        )?,
        "director_send" => {
            let director = app
                .services
                .get::<riddle_agent::director::Director>()
                .ok_or_else(|| RiddleError::llm_unavailable("the Director is not configured"))?;
            let req = riddle_agent::director::SendRequest {
                conversation_id: opt(&args, "conversationId"),
                message: arg(&args, "message")?,
                dig_ids: opt(&args, "digIds").unwrap_or_default(),
                allow_contamination: opt(&args, "allowContamination").unwrap_or(false),
                title: opt(&args, "title"),
            };
            let id = director.send(req)?;
            json!({ "conversationId": id })
        }
        "director_retry" => {
            let director = app
                .services
                .get::<riddle_agent::director::Director>()
                .ok_or_else(|| RiddleError::llm_unavailable("the Director is not configured"))?;
            let id: ConversationId = arg(&args, "conversationId")?;
            director.retry(&id)?;
            json!({ "conversationId": id })
        }
        "task_centre" => serde_json::to_value(app.task_centre()?)?,
        "get_settings" => {
            let store = app.services.require::<riddle_settings::SettingsStore>()?;
            serde_json::to_value(store.settings())?
        }
        "privacy_statement" => {
            let settings = app
                .services
                .get::<riddle_settings::SettingsStore>()
                .map(|s| s.settings());
            let judge_vendor = app.services.judge().map(|j| {
                let id = j.identity();
                format!("TypeSafe AI ({})", id.model_id)
            });
            let llm_vendor = settings
                .as_ref()
                .filter(|s| s.llm.provider == riddle_settings::LlmProvider::Bedrock)
                .map(|s| format!("Amazon Bedrock ({})", s.llm.escalation_model));
            let digs = app.list_digs()?;
            let wanted: Option<DigId> = opt(&args, "digId");
            let digs = digs
                .into_iter()
                .filter(|d| wanted.as_ref().is_none_or(|w| w == &d.id))
                .map(|d| (d.name, d.privacy))
                .collect();
            json!(riddle_privacy::disclosure(
                &riddle_privacy::DisclosureInput {
                    judge_vendor,
                    llm_vendor,
                    digs
                }
            ))
        }
        // A probe answers with the client's health after the call, so the
        // pane shows the breaker the probe just closed or tripped.
        "probe_judge" => match app.services.judge() {
            Some(j) => match j.probe().await {
                Ok(ms) => {
                    json!({ "ok": true, "latencyMs": ms, "message": j.identity().key(), "health": j.health() })
                }
                Err(e) => {
                    json!({ "ok": false, "latencyMs": 0, "message": format!("{}: {}", e.kind.as_str(), e.message), "health": j.health() })
                }
            },
            None => {
                json!({ "ok": false, "latencyMs": 0, "message": "no judge configured", "health": Value::Null })
            }
        },
        "probe_llm" => match app.services.get::<crate::ai::AiServices>() {
            Some(ai) => {
                let started = std::time::Instant::now();
                match ai.llm.probe().await {
                    Ok(_) => {
                        json!({ "ok": true, "latencyMs": started.elapsed().as_millis() as u64, "message": ai.llm.identity().model_id, "health": ai.llm.health() })
                    }
                    Err(e) => {
                        json!({ "ok": false, "latencyMs": started.elapsed().as_millis() as u64, "message": format!("{}: {}", e.kind.as_str(), e.message), "health": ai.llm.health() })
                    }
                }
            }
            None => {
                json!({ "ok": false, "latencyMs": 0, "message": "no LLM configured", "health": Value::Null })
            }
        },
        "probe_provider" => {
            let dig = app.dig(&arg(&args, "digId")?)?;
            let domains = app.services.require::<crate::domains::Domains>()?;
            let started = std::time::Instant::now();
            match domains.providers.provider_for(&dig) {
                Ok(p) => match p.profile().await {
                    Ok(profile) => {
                        // Say what the probe proved and how far sync has got,
                        // so "answered" is not read as "synced".
                        let local = app
                            .stores
                            .dig(&dig.id)
                            .and_then(|s| repo::messages::count(&s))
                            .unwrap_or(0);
                        let remote = profile
                            .messages_total
                            .map(|n| format!("the mailbox has {n} messages"))
                            .unwrap_or_else(|| "the mailbox answered".into());
                        json!({
                            "ok": true,
                            "latencyMs": started.elapsed().as_millis() as u64,
                            "message": format!("Signed in as {}; {remote}; Riddle has {local} so far", profile.email),
                        })
                    }
                    Err(e) => {
                        json!({ "ok": false, "latencyMs": started.elapsed().as_millis() as u64, "message": format!("{}: {}", e.kind.as_str(), e.message) })
                    }
                },
                Err(e) => {
                    json!({ "ok": false, "latencyMs": 0, "message": format!("{}: {}", e.kind.as_str(), e.message) })
                }
            }
        }
        "update_preview" => Value::Null,
        "compare_signal_versions" => {
            let name: String = arg(&args, "name")?;
            let a: u32 = arg(&args, "versionA")?;
            let b: u32 = arg(&args, "versionB")?;
            let mut agree = 0u64;
            let mut disagree = 0u64;
            let mut compared = 0u64;
            let mut samples = Vec::new();
            for d in app.list_digs()? {
                let store = app.stores.dig(&d.id)?;
                let ea = repo::signals::evaluations_of_signal(&store, &name, a, 5000)?;
                let eb = repo::signals::evaluations_of_signal(&store, &name, b, 5000)?;
                let by_msg: std::collections::HashMap<&MessageId, &SignalEvaluation> =
                    eb.iter().map(|e| (&e.message_id, e)).collect();
                for x in &ea {
                    if let Some(y) = by_msg.get(&x.message_id) {
                        compared += 1;
                        if x.selected == y.selected {
                            agree += 1
                        } else {
                            disagree += 1;
                            if samples.len() < 25 {
                                samples.push(json!({ "digId": d.id, "messageId": x.message_id, "a": x.selected, "aScore": x.score(None), "b": y.selected, "bScore": y.score(None) }));
                            }
                        }
                    }
                }
            }
            json!({ "name": name, "versionA": a, "versionB": b, "compared": compared, "agree": agree, "disagree": disagree, "agreement": if compared > 0 { agree as f64 / compared as f64 } else { 0.0 }, "samples": samples })
        }
        "ui_log" => {
            let level = match opt::<String>(&args, "level").as_deref() {
                Some("error") => log::Level::Error,
                Some("warn") => log::Level::Warn,
                Some("info") => log::Level::Info,
                Some("debug") => log::Level::Debug,
                _ => log::Level::Trace,
            };
            let cat = opt::<String>(&args, "category")
                .and_then(|c| riddle_observability::Category::parse(&c))
                .unwrap_or(riddle_observability::Category::Ui);
            let message: String = opt(&args, "message").unwrap_or_default();
            riddle_observability::record(level, cat.as_str(), &message);
            log::log!(target: cat.as_str(), level, "{message}");
            Value::Null
        }
        "dev_info" => dev_info(app),
        "read_app_logs" => {
            let min = opt::<String>(&args, "level").and_then(|l| l.parse::<log::Level>().ok());
            serde_json::to_value(riddle_observability::tail(
                opt::<u32>(&args, "limit").unwrap_or(500) as usize,
                opt::<String>(&args, "category").as_deref(),
                min,
                opt::<String>(&args, "text").as_deref(),
            ))?
        }
        "log_categories" => json!(riddle_observability::Category::ALL
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()),
        "export_diagnostics" => export_diagnostics(app)?,
        "clear_caches" => json!(0),
        "parse_mesh" => {
            let text: String = arg(&args, "text")?;
            let mesh = riddle_mesh::parse::parse(&text)?;
            riddle_mesh::validate::validate(
                &mesh,
                &riddle_mesh::validate::RegistryResolver {
                    index: index.clone(),
                },
            )?;
            serde_json::to_value(mesh)?
        }
        "rule_check" => serde_json::to_value(app.rule_check()?)?,
        "rule_samples" => {
            let mesh_id: MeshId = arg(&args, "meshId")?;
            let limit = opt::<usize>(&args, "limit").unwrap_or(10).min(50);
            serde_json::to_value(app.rule_samples(&mesh_id, limit)?)?
        }
        "list_packs" => serde_json::to_value(riddle_mesh::packs::catalogue(&index)?)?,
        "list_named_lists" => serde_json::to_value(repo::lists::all(&index)?)?,
        "mesh_order" => serde_json::to_value(app.mesh_order(window_days(&args, 30))?)?,
        "simulate_mesh" => {
            let window_days = window_days(&args, 90);
            let mesh_id: Option<MeshId> = opt(&args, "meshId");
            let candidate = match (opt::<String>(&args, "text"), &mesh_id) {
                (Some(text), Some(id)) => {
                    let mut m = riddle_mesh::parse::parse(&text)?;
                    let current = repo::meshes::latest(&index, id)?
                        .ok_or_else(|| RiddleError::not_found("mesh"))?;
                    m.id = current.id.clone();
                    m.version = current.version + 1;
                    if !text.to_lowercase().contains("scope:") {
                        m.scope = current.scope.clone();
                    }
                    m
                }
                (Some(text), None) => riddle_mesh::parse::parse(&text)?,
                (None, Some(id)) => {
                    let v: Option<u32> = opt(&args, "version");
                    match v {
                        Some(v) => repo::meshes::get(&index, id, v)?,
                        None => repo::meshes::latest(&index, id)?,
                    }
                    .ok_or_else(|| RiddleError::not_found("mesh"))?
                }
                (None, None) => {
                    return Err(RiddleError::invalid("simulate_mesh needs text or meshId"))
                }
            };
            let digs = app.list_digs()?;
            let mut input =
                riddle_mesh::simulate::SimulationInput::new(&app.stores, digs, &candidate);
            input.window_days = window_days;
            serde_json::to_value(riddle_mesh::simulate::simulate(input)?)?
        }
        other => return Err(RiddleError::not_found(format!("no command named {other}"))),
    })
}

/// Trace statistics over the given Digs since a moment, averages weighted
/// by each Dig's count. Shared by `trace_stats` and the diagnostics export.
fn trace_stats(
    app: &App,
    digs: &[Dig],
    since: &chrono::DateTime<chrono::Utc>,
) -> Result<repo::traces::TraceStats> {
    let mut total = repo::traces::TraceStats::default();
    let mut n = 0u64;
    for d in digs {
        let store = app.stores.dig(&d.id)?;
        let s = repo::traces::stats(&store, since)?;
        total.count += s.count;
        total.escalated += s.escalated;
        total.unresolved += s.unresolved;
        total.judge_tokens += s.judge_tokens;
        total.judge_cost_usd += s.judge_cost_usd;
        total.llm_cost_usd += s.llm_cost_usd;
        total.avg_total_ms += s.avg_total_ms * s.count as f64;
        total.avg_judge_ms += s.avg_judge_ms * s.count as f64;
        n += s.count;
    }
    if n > 0 {
        total.avg_total_ms /= n as f64;
        total.avg_judge_ms /= n as f64;
    }
    Ok(total)
}

/// The Developer pane's runtime row: build, directories, store sizes, the
/// fault flag, and the swallowed-failure table (`riddle_observability`).
fn dev_info(app: &App) -> Value {
    let mut stores = vec![
        json!({ "label": "index", "path": app.stores.index().path().map(|p| p.display().to_string()).unwrap_or_default(), "sizeBytes": app.stores.index().size_bytes() }),
    ];
    for (id, s) in app.stores.open_digs() {
        stores.push(json!({ "label": format!("dig {id}"), "path": s.path().map(|p| p.display().to_string()).unwrap_or_default(), "sizeBytes": s.size_bytes() }));
    }
    json!({
        "build": app.config.build, "dev": app.config.dev, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "uptimeSeconds": app.started_at.elapsed().as_secs(),
        "dataDir": app.config.data_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        "logDir": app.config.data_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        "configDir": app.config.config_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        "cacheDir": "", "stores": stores, "storeFault": app.stores.any_fault(), "logLevel": log::max_level().to_string(),
        "swallowed": riddle_observability::swallowed_failures(),
    })
}

/// Settings as the export carries them. The settings file holds no secret
/// (presence booleans only, §12), and the Gmail OAuth client id, which is
/// not one either, still goes out as configured-or-absent: the export is
/// for pasting into an issue, and the id has no diagnostic value.
fn redacted_settings(s: &riddle_settings::Settings) -> Value {
    let mut v = serde_json::to_value(s).unwrap_or(Value::Null);
    if let Some(g) = v.get_mut("gmail").and_then(|g| g.as_object_mut()) {
        for key in ["clientId", "client_id"] {
            if let Some(id) = g.get_mut(key) {
                let configured = id.as_str().is_some_and(|s| !s.trim().is_empty());
                *id = json!(if configured { "configured" } else { "absent" });
            }
        }
    }
    v
}

/// The redacted diagnostics bundle (`APPLICATION_FEATURES.md` §4 item 5):
/// one JSON document with the runtime, the Task Centre, trace statistics,
/// the judge and LLM identity and health, every account's status with its
/// reason, the settings, the swallowed-failure table and the warning tail.
/// Identifiers only: no subjects, bodies, addresses, names, tokens or keys.
/// It used to be the version, a Dig count and fifty warn lines, which was
/// not enough to see that the judge's breaker had been open for a fortnight.
fn export_diagnostics(app: &Arc<App>) -> Result<Value> {
    let now = chrono::Utc::now();
    let digs = app.list_all_digs()?;
    let dig_rows: Vec<Value> = digs
        .iter()
        .map(|d| {
            json!({
                "id": d.id, "name": d.name, "provider": d.provider, "privacy": d.privacy,
                "projection": d.projection, "disabled": d.disabled, "status": d.status,
            })
        })
        .collect();
    let enabled: Vec<Dig> = digs.iter().filter(|d| !d.disabled).cloned().collect();
    let judge = app
        .services
        .judge()
        .map(|j| json!({ "identity": j.identity(), "health": j.health() }));
    // `llm_health` is `None` under the `null` provider, whose client is a
    // placeholder, so the export says there is no LLM rather than naming it.
    let llm = app.llm_health().and_then(|health| {
        app.services
            .get::<crate::ai::AiServices>()
            .map(|ai| json!({ "identity": ai.llm.identity(), "health": health }))
    });
    let settings = app
        .services
        .get::<riddle_settings::SettingsStore>()
        .map(|s| redacted_settings(&s.settings()));
    let warnings: Vec<Value> = riddle_observability::tail(50, None, Some(log::Level::Warn), None)
        .into_iter()
        .map(|l| json!({ "at": l.at, "level": l.level, "category": l.category, "message": l.message }))
        .collect();
    Ok(json!({
        "exportedAt": now,
        "redaction": "identifiers only: no subjects, bodies, addresses, names, tokens or keys",
        "runtime": dev_info(app),
        "taskCentre": app.task_centre()?,
        "traceStats": trace_stats(app, &enabled, &(now - chrono::Duration::days(30)))?,
        "judge": judge,
        "llm": llm,
        "digs": dig_rows,
        "actions": app.runtime.list_actions().len(),
        "settings": settings,
        "swallowed": riddle_observability::swallowed_failures(),
        "warnings": warnings,
    }))
}
