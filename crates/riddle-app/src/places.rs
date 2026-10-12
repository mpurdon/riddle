//! The Inbox's mail column (`riddle_core::places`): which places there are,
//! with counts, and what mail each place means in each account. The column
//! is option D of the mail-navigation study (11 Oct 2026): mailboxes, the
//! person's pins, Topics (a Stratum and the person's label of the same name,
//! one row), then the rest of their labels as a tree. Labels are matched by
//! name, not id, so one place covers every account; a Dig's own label ids
//! are looked up when its mail is listed. Pins are the person's choice and
//! live in the index store (`mail.pinned`), changed only by the
//! `pin_place` and `unpin_place` Actions.

use crate::App;
use riddle_actions::{ActionContext, ActionResult};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use riddle_store::repo::messages::{MessageFilter, PriorityCriteria};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// The pins, a JSON array of place strings in the order they were pinned.
pub const PINNED_KEY: &str = "mail.pinned";

/// Gmail's mailboxes the column always shows, in Gmail's order.
const MAILBOXES: &[(&str, &str)] = &[("INBOX", "Inbox"), ("STARRED", "Starred"), ("SENT", "Sent")];

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

pub fn pinned(store: &riddle_store::Store) -> Result<Vec<Place>> {
    let raw: Vec<String> = repo::kv::get_json(store, PINNED_KEY)?.unwrap_or_default();
    Ok(raw.into_iter().filter_map(|s| s.parse().ok()).collect())
}

fn row(place: Place, name: impl Into<String>, (messages, unread): (u64, u64)) -> MailPlace {
    MailPlace {
        kind: place.kind(),
        place,
        name: name.into(),
        has_label: false,
        messages,
        unread,
        pinned: false,
    }
}

impl App {
    /// The mail column for one account, or every enabled one.
    pub fn mail_places(&self, dig_id: Option<&DigId>) -> Result<MailPlaces> {
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => self.list_digs()?,
        };
        let index = self.stores.index();
        let labels = self.list_labels(dig_id)?;
        let add = |a: &mut (u64, u64), b: (u64, u64)| {
            a.0 += b.0;
            a.1 += b.1;
        };

        // Mailboxes: all mail, then Gmail's own, summed over the accounts.
        let mut all = (0, 0);
        for d in &digs {
            let store = self.stores.dig(&d.id)?;
            all.0 += repo::messages::count(&store)?;
            all.1 += repo::provider_labels::counts(&store)?
                .get("UNREAD")
                .map_or(0, |c| c.0);
        }
        let mut mailboxes = vec![row(Place::All, "All mail", all)];
        for (id, name) in MAILBOXES {
            let mut n = (0, 0);
            for l in labels.iter().filter(|l| l.id == *id) {
                add(&mut n, (l.messages, l.unread));
            }
            mailboxes.push(row(Place::Mailbox(id.to_string()), *name, n));
        }

        // The person's own labels, by full name across the accounts.
        let mut own: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for l in labels.iter().filter(|l| l.kind == LabelKind::User) {
            add(
                own.entry(l.name.clone()).or_default(),
                (l.messages, l.unread),
            );
        }

        // Topics: each Stratum with mail, merged with any label of its name.
        let mut stratum_counts: HashMap<String, (u64, u64)> = HashMap::new();
        for d in &digs {
            for (s, n) in repo::messages::strata_counts(&*self.stores.dig(&d.id)?)? {
                add(stratum_counts.entry(s).or_default(), n);
            }
        }
        let pins = pinned(&index)?;
        let mut topics = Vec::new();
        for s in repo::strata::list(&index, false)? {
            let merged: Vec<String> = own
                .keys()
                .filter(|n| same_name(n, &s.name))
                .cloned()
                .collect();
            let place = Place::Topic(s.name.clone());
            let n = if merged.is_empty() {
                stratum_counts.get(&s.name).copied().unwrap_or_default()
            } else {
                let mut n = (0, 0);
                for d in &digs {
                    let crit = self.place_criteria(&d.id, &place)?.unwrap_or_default();
                    add(
                        &mut n,
                        repo::messages::count_any_of(
                            &*self.stores.dig(&d.id)?,
                            &crit.strata,
                            &crit.labels,
                        )?,
                    );
                }
                n
            };
            if n.0 == 0 && merged.is_empty() && !pins.contains(&place) {
                continue;
            }
            for m in &merged {
                own.remove(m);
            }
            topics.push(MailPlace {
                has_label: !merged.is_empty(),
                ..row(place, s.name, n)
            });
        }
        topics.sort_by(|a, b| {
            b.messages
                .cmp(&a.messages)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        // The rest of the labels in tree order: a child after its parent,
        // Gmail's legacy `[Imap]/…` folders last.
        let mut rest: Vec<MailPlace> = own
            .into_iter()
            .map(|(name, n)| row(Place::Label(name.clone()), name, n))
            .collect();
        rest.sort_by_key(|l| (l.name.starts_with('['), l.name.to_lowercase()));

        // Pins move their place out of its section, in the order pinned.
        let mut pinned_rows = Vec::new();
        for p in pins {
            for section in [&mut mailboxes, &mut topics, &mut rest] {
                if let Some(i) = section.iter().position(|x| x.place == p) {
                    pinned_rows.push(MailPlace {
                        pinned: true,
                        ..section.remove(i)
                    });
                    break;
                }
            }
        }
        Ok(MailPlaces {
            mailboxes,
            pinned: pinned_rows,
            topics,
            labels: rest,
        })
    }

    /// What a place means in one account, for the message query: `None`
    /// for all mail. A label this account does not have matches nothing.
    pub fn place_criteria(&self, dig: &DigId, place: &Place) -> Result<Option<PriorityCriteria>> {
        let store = self.stores.dig(dig)?;
        let ids_named = |pred: &dyn Fn(&str) -> bool| -> Result<Vec<String>> {
            let named = repo::provider_labels::all(&store)?;
            let mut ids: Vec<String> = named
                .iter()
                .filter(|l| pred(&l.name))
                .map(|l| l.id.clone())
                .collect();
            // A label on stored mail the account's list has not named yet goes by its id.
            if named.is_empty() || ids.is_empty() {
                ids.extend(
                    repo::provider_labels::counts(&store)?
                        .into_keys()
                        .filter(|id| pred(id)),
                );
            }
            Ok(ids)
        };
        Ok(match place {
            Place::All => None,
            Place::Mailbox(id) => Some(PriorityCriteria {
                labels: vec![id.clone()],
                ..Default::default()
            }),
            Place::Label(name) => Some(PriorityCriteria {
                labels: ids_named(&|n| n == name)?,
                ..Default::default()
            }),
            Place::Topic(name) => {
                let strata = repo::strata::list(&self.stores.index(), false)?
                    .into_iter()
                    .filter(|s| same_name(&s.name, name))
                    .map(|s| s.name)
                    .collect();
                Some(PriorityCriteria {
                    strata,
                    labels: ids_named(&|n| same_name(n, name))?,
                    ..Default::default()
                })
            }
        })
    }

    /// The Inbox at a place: the filter, with the place resolved per account.
    pub fn list_messages_at(
        &self,
        dig_id: Option<&DigId>,
        filter: &MessageFilter,
        place: Option<&Place>,
        page: &PageRequest,
    ) -> Result<Page<MessageSummary>> {
        let Some(place) = place else {
            return self.list_messages(dig_id, filter, page);
        };
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => self.list_digs()?,
        };
        let mut per_dig = HashMap::new();
        for d in &digs {
            per_dig.insert(d.id.clone(), self.place_criteria(&d.id, place)?);
        }
        self.merge_messages(
            &digs,
            |d| MessageFilter {
                any_of: per_dig.get(&d.id).cloned().flatten(),
                ..filter.clone()
            },
            page,
        )
    }
}

fn place_arg(args: &Value) -> Result<Place> {
    args.get("place")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RiddleError::invalid("missing argument place"))?
        .parse()
}

fn save(ctx: &ActionContext, pins: &[Place]) -> Result<()> {
    let raw: Vec<String> = pins.iter().map(|p| p.to_string()).collect();
    repo::kv::set_json(&ctx.services.stores.index(), PINNED_KEY, &raw)
}

fn place_label(p: &Place) -> String {
    match p {
        Place::All => "All mail".into(),
        Place::Mailbox(id) => MAILBOXES
            .iter()
            .find(|(m, _)| m == id)
            .map_or(id.clone(), |(_, n)| n.to_string()),
        Place::Label(n) | Place::Topic(n) => n.clone(),
    }
}

/// `pin_place`: keep a place at the top of the mail column.
pub struct PinPlace;

#[async_trait::async_trait]
impl riddle_actions::Action for PinPlace {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "pin_place",
            "Pin a label, Topic or mailbox to the top of the mail column",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let place = place_arg(&args)?;
        let mut pins = pinned(&ctx.services.stores.index())?;
        if !pins.contains(&place) {
            pins.push(place.clone());
            save(ctx, &pins)?;
        }
        Ok(ActionResult::ok(
            json!({ "place": place }),
            format!("Pinned {}", place_label(&place)),
        )
        .with_undo(json!({ "place": place, "pin": false }), "Unpin"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        unpin(ctx, &place_arg(&restore)?)
    }
}

/// `unpin_place`: return a pinned place to its section.
pub struct UnpinPlace;

#[async_trait::async_trait]
impl riddle_actions::Action for UnpinPlace {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta("unpin_place", "Unpin a place from the mail column")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let place = place_arg(&args)?;
        let pins = pinned(&ctx.services.stores.index())?;
        let at = pins.iter().position(|p| *p == place);
        let r = unpin(ctx, &place)?;
        Ok(r.with_undo(json!({ "place": place, "at": at }), "Pin again"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let place = place_arg(&restore)?;
        let mut pins = pinned(&ctx.services.stores.index())?;
        if !pins.contains(&place) {
            let at = restore
                .get("at")
                .and_then(|v| v.as_u64())
                .map_or(pins.len(), |i| (i as usize).min(pins.len()));
            pins.insert(at, place.clone());
            save(ctx, &pins)?;
        }
        Ok(ActionResult::ok(
            json!({ "place": place }),
            format!("Pinned {}", place_label(&place)),
        ))
    }
}

fn unpin(ctx: &ActionContext, place: &Place) -> Result<ActionResult> {
    let mut pins = pinned(&ctx.services.stores.index())?;
    pins.retain(|p| p != place);
    save(ctx, &pins)?;
    Ok(ActionResult::ok(
        json!({ "place": place }),
        format!("Unpinned {}", place_label(place)),
    ))
}

pub fn actions() -> Vec<Arc<dyn riddle_actions::Action>> {
    vec![Arc::new(PinPlace), Arc::new(UnpinPlace)]
}
