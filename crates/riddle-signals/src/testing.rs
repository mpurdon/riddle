//! Signal testing before promotion (§31, §32, §34). A candidate runs in an
//! isolated experimental evaluation: nothing it produces enters the cache or
//! a Classification (invariant 17). The corpus is built from current
//! classifications through an explicit mapping from disposition to the
//! expected Signal value, plus active corrections, plus the Signal's stored
//! adversarial examples; the report names the dataset and the engine
//! snapshot so two reports are compared only within one snapshot.

use crate::calibration::LabelledSet;
use crate::sets::{SetId, StateBuilder, StateInput};
use riddle_core::{
    Calibration, DigId, Disposition, DispositionKind, FindType, MessageId, SemanticJudge, Signal,
};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Store, Stores};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The value a Signal is expected to take given a message's disposition, or
/// `None` when the disposition says nothing about it. This mapping defines
/// the corpus and is therefore part of the methodology the report names.
pub fn expected_for(signal_name: &str, disposition: &Disposition) -> Option<bool> {
    use Disposition::*;
    let find = |t: FindType| matches!(disposition, Find { find_type } if *find_type == t);
    let hidden = matches!(disposition, Spoil);
    let attention = matches!(disposition, Find { .. });
    match signal_name {
        "requires_response" | "asks_question" => {
            (attention || hidden).then(|| find(FindType::Respond))
        }
        "requires_action" | "requests_action" | "direct_request" => {
            (attention || hidden).then(|| {
                find(FindType::Do)
                    || find(FindType::Respond)
                    || find(FindType::Pay)
                    || find(FindType::Review)
            })
        }
        "review_requested" => (attention || hidden).then(|| find(FindType::Review)),
        "ci_failure" | "deployment_failed" | "owned_work_failed" => {
            (attention || hidden).then(|| find(FindType::Do))
        }
        "meeting_related" => (attention || hidden).then(|| find(FindType::Attend)),
        "payment_related" | "invoice" => (attention || hidden).then(|| find(FindType::Pay)),
        "security_alert" => (attention || hidden).then(|| find(FindType::Know)),
        "personally_relevant" | "human_authored" => (attention || hidden).then_some(attention),
        "routine" | "marketing" | "newsletter" | "social_notification" => {
            (attention || hidden).then_some(hidden)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorpusItem {
    pub message_id: MessageId,
    pub dig_id: DigId,
    pub expected: Option<bool>,
    /// `classification`, `correction` or `adversarial`.
    pub source: String,
    /// Text for adversarial items, which have no stored message.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignalCorpus {
    pub items: Vec<CorpusItem>,
    pub positives: u32,
    pub negatives: u32,
    pub corrected: u32,
    pub adversarial: u32,
}

/// Build the corpus for one Dig: positives and negatives from current
/// classifications (newest first, then a seeded sample), corrections on top.
pub fn build_corpus(
    dig_store: &Store,
    dig_id: &DigId,
    signal_name: &str,
    sample_size: u32,
    seed: u64,
) -> Result<SignalCorpus> {
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut corpus = SignalCorpus::default();
    let per_side = (sample_size / 2).max(1);
    let mut positives = Vec::new();
    let mut negatives = Vec::new();
    for kind in [
        DispositionKind::Find,
        DispositionKind::Spoil,
        DispositionKind::Filed,
    ] {
        for id in repo::classifications::ids_with_kind(dig_store, kind, sample_size * 4)? {
            let Some(k) = repo::classifications::current(dig_store, &id)? else {
                continue;
            };
            match expected_for(signal_name, &k.disposition) {
                Some(true) => positives.push(id),
                Some(false) => negatives.push(id),
                None => {}
            }
        }
    }
    negatives.shuffle(&mut rng);
    positives.truncate(per_side as usize);
    negatives.truncate(per_side as usize);
    corpus.positives = positives.len() as u32;
    corpus.negatives = negatives.len() as u32;
    for id in positives {
        corpus.items.push(CorpusItem {
            message_id: id,
            dig_id: dig_id.clone(),
            expected: Some(true),
            source: "classification".into(),
            text: None,
        });
    }
    for id in negatives {
        corpus.items.push(CorpusItem {
            message_id: id,
            dig_id: dig_id.clone(),
            expected: Some(false),
            source: "classification".into(),
            text: None,
        });
    }
    for k in repo::corrections::active_moves(dig_store, sample_size)? {
        let Some(to) = &k.to_disposition else {
            continue;
        };
        let Some(expected) = expected_for(signal_name, to) else {
            continue;
        };
        if corpus.items.iter().any(|i| i.message_id == k.message_id) {
            continue;
        }
        corpus.items.push(CorpusItem {
            message_id: k.message_id.clone(),
            dig_id: dig_id.clone(),
            expected: Some(expected),
            source: "correction".into(),
            text: None,
        });
        corpus.corrected += 1;
    }
    Ok(corpus)
}

/// A permanent semantic regression test (§32).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdversarialExample {
    pub signal_name: String,
    pub text: String,
    pub expected: bool,
    pub note: String,
}

fn adversarial_key(name: &str) -> String {
    format!("adversarial:{name}")
}

pub fn store_adversarial(index: &Store, example: AdversarialExample) -> Result<usize> {
    let mut all = load_adversarial(index, &example.signal_name)?;
    if !all.iter().any(|e| e.text == example.text) {
        all.push(example.clone());
    }
    repo::kv::set_json(index, &adversarial_key(&example.signal_name), &all)?;
    Ok(all.len())
}

pub fn load_adversarial(index: &Store, name: &str) -> Result<Vec<AdversarialExample>> {
    Ok(repo::kv::get_json(index, &adversarial_key(name))?.unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestSample {
    pub message_id: MessageId,
    pub dig_id: DigId,
    pub expected: Option<bool>,
    pub got: String,
    pub confidence: f64,
    pub raw_yes: f64,
    pub source: String,
}

/// The shape `docs/COMMANDS.md` specifies for `test_signal`, plus the
/// methodology fields §31 requires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalTestReport {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub definition: String,
    pub sample_size: u32,
    /// Agreement with expected labels over the labelled items.
    pub agreement: f64,
    pub false_positives: u32,
    pub false_negatives: u32,
    /// Items with |p − 0.5| < 0.15.
    pub uncertain: u32,
    pub latency_ms: u64,
    pub cost_usd: Option<f64>,
    /// Confidence histogram: bucket label to count.
    pub distribution: BTreeMap<String, u32>,
    pub samples: Vec<TestSample>,
    pub positives: u32,
    pub negatives: u32,
    pub corrected: u32,
    pub adversarial_failures: u32,
    pub engine_identity: String,
    pub calibration: Option<Calibration>,
    pub tested_at: chrono::DateTime<chrono::Utc>,
}

fn bucket(p: f64) -> String {
    let b = ((p * 10.0).floor() as u32).min(9);
    format!("{:.1}-{:.1}", b as f64 / 10.0, (b + 1) as f64 / 10.0)
}

/// Run a Signal (any status) against the corpus of the given Digs. Nothing is
/// cached; the report is stored as an experiment and returned.
pub async fn test_signal(
    stores: &Stores,
    digs: &[DigId],
    judge: &dyn SemanticJudge,
    signal: &Signal,
    sample_size: u32,
) -> Result<SignalTestReport> {
    let index = stores.index();
    let mut items: Vec<CorpusItem> = Vec::new();
    let (mut positives, mut negatives, mut corrected) = (0, 0, 0);
    for dig in digs {
        let store = stores.dig(dig)?;
        let corpus = build_corpus(&store, dig, &signal.name, sample_size, 42)?;
        positives += corpus.positives;
        negatives += corpus.negatives;
        corrected += corpus.corrected;
        items.extend(corpus.items);
    }
    let adversarial = load_adversarial(&index, &signal.name)?;
    for (i, a) in adversarial.iter().enumerate() {
        items.push(CorpusItem {
            message_id: MessageId(format!("adversarial:{i}")),
            dig_id: DigId("adversarial".into()),
            expected: Some(a.expected),
            source: "adversarial".into(),
            text: Some(a.text.clone()),
        });
    }
    if items.is_empty() {
        return Err(RiddleError::invalid(format!(
            "no corpus for {}: classify some mail or add adversarial examples first",
            signal.name
        )));
    }
    let builder = StateBuilder::default();
    let engine = judge.identity().key();
    let price = judge.identity().price_per_mtok;
    let mut samples = Vec::new();
    let mut latency = 0u64;
    let mut tokens = 0u64;
    let mut labelled = LabelledSet::new(&signal.name, signal.version);
    for item in &items {
        let input = match &item.text {
            Some(t) => StateInput {
                subject: String::new(),
                from: String::new(),
                normalized_text: t.clone(),
                ..Default::default()
            },
            None => {
                let store = stores.dig(&item.dig_id)?;
                let Some(m) = repo::messages::get(&store, &item.dig_id, &item.message_id)? else {
                    continue;
                };
                StateInput {
                    subject: m.subject.clone(),
                    from: m.from.email.clone(),
                    normalized_text: m.body_text.clone().unwrap_or_else(|| m.snippet.clone()),
                    ..Default::default()
                }
            }
        };
        let built = builder.build(SetId::Baseline, std::slice::from_ref(signal), &input);
        let started = std::time::Instant::now();
        let answers = judge
            .evaluate(&built.state, &[crate::cache::question_for(signal)])
            .await?;
        latency += started.elapsed().as_millis() as u64;
        let Some(a) = answers.first() else { continue };
        tokens += a.input_tokens;
        let e = crate::cache::evaluation_from(
            signal,
            &item.message_id,
            &engine,
            a.distribution.clone(),
            a.latency_ms,
            a.input_tokens,
            a.deterministic,
            None,
            None,
            false,
        );
        let raw_yes = e.distribution.get("YES").copied().unwrap_or(0.0);
        if let Some(label) = item.expected {
            labelled.pairs.push((raw_yes, label));
        }
        samples.push(TestSample {
            message_id: item.message_id.clone(),
            dig_id: item.dig_id.clone(),
            expected: item.expected,
            got: e.selected.clone(),
            confidence: e.confidence,
            raw_yes,
            source: item.source.clone(),
        });
    }
    let labelled_n = samples.iter().filter(|s| s.expected.is_some()).count() as f64;
    let mut agree = 0u32;
    let (mut fp, mut fn_, mut uncertain, mut adversarial_failures) = (0, 0, 0, 0);
    let mut distribution = BTreeMap::new();
    for s in &samples {
        *distribution.entry(bucket(s.raw_yes)).or_insert(0) += 1;
        if (s.raw_yes - 0.5).abs() < 0.15 {
            uncertain += 1;
        }
        let got_yes = s.got == "YES";
        match s.expected {
            Some(true) if got_yes => agree += 1,
            Some(false) if !got_yes => agree += 1,
            Some(true) => {
                fn_ += 1;
                if s.source == "adversarial" {
                    adversarial_failures += 1;
                }
            }
            Some(false) => {
                fp += 1;
                if s.source == "adversarial" {
                    adversarial_failures += 1;
                }
            }
            None => {}
        }
    }
    let calibration = if labelled.pairs.len() >= 20 {
        Some(labelled.calibrate(
            &engine,
            Some(if labelled_n > 0.0 {
                agree as f64 / labelled_n
            } else {
                0.0
            }),
        ))
    } else {
        None
    };
    let report = SignalTestReport {
        id: uuid::Uuid::now_v7().to_string(),
        name: signal.name.clone(),
        version: signal.version,
        definition: signal.definition.clone(),
        sample_size: samples.len() as u32,
        agreement: if labelled_n > 0.0 {
            agree as f64 / labelled_n
        } else {
            0.0
        },
        false_positives: fp,
        false_negatives: fn_,
        uncertain,
        latency_ms: latency,
        cost_usd: price.map(|p| tokens as f64 * p / 1_000_000.0),
        distribution,
        samples,
        positives,
        negatives,
        corrected,
        adversarial_failures,
        engine_identity: engine,
        calibration,
        tested_at: chrono::Utc::now(),
    };
    if let Err(e) = store_report(&index, &report) {
        // Bookkeeping never fails the work.
        riddle_observability::rwarn!(
            Category::Sift,
            "could not store test report for {}@v{}: {}",
            report.name,
            report.version,
            e.message
        );
    }
    rinfo!(
        Category::Sift,
        "tested {}@v{}: {} samples, agreement {:.2}, {} uncertain",
        report.name,
        report.version,
        report.sample_size,
        report.agreement,
        report.uncertain
    );
    Ok(report)
}

fn reports_key(name: &str) -> String {
    format!("signal_tests:{name}")
}

/// Reports are kept as experiments in the index store, newest last, capped.
pub fn store_report(index: &Store, report: &SignalTestReport) -> Result<()> {
    let mut all = list_reports(index, &report.name)?;
    all.push(report.clone());
    if all.len() > 20 {
        let drop = all.len() - 20;
        all.drain(0..drop);
    }
    repo::kv::set_json(index, &reports_key(&report.name), &all)
}

pub fn list_reports(index: &Store, name: &str) -> Result<Vec<SignalTestReport>> {
    Ok(repo::kv::get_json(index, &reports_key(name))?.unwrap_or_default())
}

/// The newest report for a Signal version, which promotion asks for.
pub fn latest_report(index: &Store, name: &str, version: u32) -> Result<Option<SignalTestReport>> {
    Ok(list_reports(index, name)?
        .into_iter()
        .rev()
        .find(|r| r.version == version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::system_signal;
    use riddle_core::*;
    use riddle_judge::ScriptedJudge;

    fn message(dig: &DigId, pid: &str, subject: &str, body: &str) -> Message {
        Message {
            id: MessageId::for_provider(dig, pid),
            dig_id: dig.clone(),
            thread_id: ThreadId::for_provider(dig, pid),
            provider_id: pid.into(),
            subject: subject.into(),
            from: Address::new("notifications@github.com", Some("GitHub")),
            to: vec![],
            cc: vec![],
            bcc: vec![],
            date: chrono::Utc::now(),
            snippet: body.into(),
            labels: vec![],
            headers: MessageHeaders::default(),
            attachments: vec![],
            size_bytes: 0,
            is_sent: false,
            has_body: true,
            body_text: Some(body.into()),
            body_html: None,
            synced_at: chrono::Utc::now(),
        }
    }

    fn classify(store: &Store, m: &Message, d: Disposition) {
        let k = Classification {
            id: ClassificationId::new(),
            message_id: m.id.clone(),
            dig_id: m.dig_id.clone(),
            importance: Importance::default_for(&d),
            disposition: d,
            strata: vec![],
            notify: false,
            mechanism: DecisionMechanism::Mesh,
            mesh_id: None,
            mesh_version: None,
            matched_rule: None,
            confidence: None,
            reason: String::new(),
            trace_id: TraceId::new(),
            shadow: false,
            created_at: chrono::Utc::now(),
            superseded_at: None,
            series: None,
        };
        repo::classifications::record(store, &k).unwrap();
    }

    #[tokio::test]
    async fn a_test_run_builds_the_corpus_from_classifications_and_reports_agreement() {
        let stores = Stores::in_memory().unwrap();
        let dig = DigId("d1".into());
        let store = stores.dig(&dig).unwrap();
        let pos = message(
            &dig,
            "p1",
            "review requested: PR #4",
            "Please review this pull request.",
        );
        let neg = message(&dig, "n1", "[ci] nightly passed", "All checks passed.");
        repo::messages::upsert(&store, &pos).unwrap();
        repo::messages::upsert(&store, &neg).unwrap();
        classify(
            &store,
            &pos,
            Disposition::Find {
                find_type: FindType::Review,
            },
        );
        classify(&store, &neg, Disposition::Spoil);
        store_adversarial(
            &stores.index(),
            AdversarialExample {
                signal_name: "review_requested".into(),
                text: "No review requested, just FYI.".into(),
                expected: false,
                note: "mentions review in passing".into(),
            },
        )
        .unwrap();
        let judge = ScriptedJudge::default();
        let signal = system_signal("review_requested").unwrap();
        let report = test_signal(&stores, std::slice::from_ref(&dig), &judge, &signal, 10)
            .await
            .unwrap();
        assert_eq!(report.sample_size, 3);
        assert_eq!(report.positives, 1);
        assert_eq!(report.negatives, 1);
        assert!(report.agreement >= 2.0 / 3.0, "{}", report.agreement);
        assert_eq!(
            latest_report(&stores.index(), "review_requested", 1)
                .unwrap()
                .unwrap()
                .id,
            report.id
        );
        // Experimental evaluation never enters the cache.
        assert_eq!(repo::signals::evaluation_count(&store).unwrap(), 0);
    }
}
