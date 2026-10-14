//! Test doubles. `NullJudge` is the "no judge configured" case and must make
//! Sift produce Unresolved without hiding anything (invariant 37).
//! `ScriptedJudge` answers from keywords so tests and Demo Mode run without
//! the network; its answers are deterministic and its call count is
//! observable, which is how the cache tests prove one call was made.

use async_trait::async_trait;
use parking_lot::Mutex;
use riddle_core::{
    Distribution, JudgeAnswer, JudgeIdentity, JudgeLimits, JudgeQuestion, JudgeState,
    SemanticJudge, SignalPrimitive,
};
use riddle_errors::{Result, RiddleError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct NullJudge;

#[async_trait]
impl SemanticJudge for NullJudge {
    async fn evaluate(
        &self,
        _state: &JudgeState,
        _questions: &[JudgeQuestion],
    ) -> Result<Vec<JudgeAnswer>> {
        Err(RiddleError::judge_unavailable("no judge configured"))
    }
    fn limits(&self) -> JudgeLimits {
        JudgeLimits {
            max_state_tokens: 32_000,
            max_request_tokens: 64_000,
            max_questions: 64,
            requests_per_minute: 0,
        }
    }
    fn identity(&self) -> JudgeIdentity {
        JudgeIdentity {
            provider: "null".into(),
            model_id: "null".into(),
            model_version: "0".into(),
            price_per_mtok: None,
        }
    }
}

#[derive(Default)]
pub struct ScriptedJudge {
    /// Signal name to fixed YES probability, overriding the keyword rules.
    overrides: Mutex<HashMap<String, f64>>,
    calls: AtomicUsize,
    unavailable: AtomicBool,
    last_state: Mutex<Option<String>>,
}

impl ScriptedJudge {
    pub fn new() -> Self {
        Self::default()
    }
    /// Force a Signal's YES probability regardless of the text.
    pub fn with_override(self, signal_name: &str, yes: f64) -> Self {
        self.overrides.lock().insert(signal_name.to_string(), yes);
        self
    }
    pub fn set_override(&self, signal_name: &str, yes: f64) {
        self.overrides.lock().insert(signal_name.to_string(), yes);
    }
    /// Simulate an outage: every call returns `JudgeUnavailable`.
    pub fn set_unavailable(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::SeqCst);
    }
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    pub fn last_state(&self) -> Option<String> {
        self.last_state.lock().clone()
    }

    fn any(text: &str, needles: &[&str]) -> bool {
        needles.iter().any(|n| text.contains(n))
    }

    /// The keyword rules. Written so the doc's GitHub example ("review
    /// requested") lands on Find: Review and a newsletter on Spoil.
    pub fn yes_probability(signal: &str, text: &str) -> f64 {
        let t = text.to_lowercase();
        let automated = Self::any(
            &t,
            &[
                "unsubscribe",
                "no-reply",
                "noreply",
                "notifications@",
                "do not reply",
                "automated",
            ],
        );
        let question = t.contains('?');
        match signal {
            "review_requested" => {
                if Self::any(
                    &t,
                    &[
                        "review requested",
                        "requested your review",
                        "please review",
                        "requested a review",
                    ],
                ) {
                    0.96
                } else {
                    0.04
                }
            }
            "ci_failure" => {
                if Self::any(&t, &["passed", "succeeded", "success"]) && !t.contains("fail") {
                    0.03
                } else if t.contains("fail")
                    && Self::any(&t, &["ci", "build", "check", "workflow", "test", "job"])
                {
                    0.95
                } else {
                    0.05
                }
            }
            "deployment_failed" => {
                if Self::any(&t, &["deploy", "rollout", "release"]) && t.contains("fail") {
                    0.93
                } else {
                    0.04
                }
            }
            "owned_work_failed" => {
                if t.contains("your") && t.contains("fail") {
                    0.85
                } else {
                    0.1
                }
            }
            "asks_question" => {
                if question {
                    0.9
                } else {
                    0.08
                }
            }
            "newsletter" => {
                if Self::any(&t, &["newsletter", "this week in", "digest", "issue #"]) {
                    0.97
                } else if t.contains("unsubscribe") && !t.contains("order") {
                    0.9
                } else {
                    0.06
                }
            }
            "marketing" => {
                if Self::any(
                    &t,
                    &[
                        "% off",
                        "sale",
                        "offer",
                        "deal",
                        "discount",
                        "limited time",
                        "shop now",
                    ],
                ) || t.contains("unsubscribe")
                {
                    0.9
                } else {
                    0.07
                }
            }
            "human_authored" => {
                if automated {
                    0.05
                } else if question
                    || Self::any(&t, &["thanks", "hi ", "hello", "cheers", "regards"])
                {
                    0.9
                } else {
                    0.5
                }
            }
            "routine" => {
                if Self::any(
                    &t,
                    &[
                        "notification",
                        "digest",
                        "weekly",
                        "daily",
                        "report",
                        "unsubscribe",
                        "passed",
                        "merged",
                    ],
                ) {
                    0.88
                } else {
                    0.2
                }
            }
            "novel" => 0.3,
            "personally_relevant" => {
                if Self::any(
                    &t,
                    &[
                        "@you",
                        "you were",
                        "your ",
                        "requested your",
                        "assigned to you",
                    ],
                ) {
                    0.85
                } else {
                    0.25
                }
            }
            "requires_response" => {
                if question
                    || Self::any(
                        &t,
                        &[
                            "let me know",
                            "please reply",
                            "can you",
                            "could you",
                            "would you",
                            "get back to me",
                        ],
                    )
                {
                    0.9
                } else {
                    0.1
                }
            }
            "requires_action" => {
                if Self::any(
                    &t,
                    &[
                        "please",
                        "action required",
                        "need you to",
                        "by friday",
                        "by eod",
                        "due",
                    ],
                ) {
                    0.85
                } else {
                    0.12
                }
            }
            "direct_request" => {
                if Self::any(&t, &["can you", "could you", "please "]) {
                    0.85
                } else {
                    0.1
                }
            }
            "makes_commitment" => {
                if Self::any(&t, &["i will", "i'll", "will send", "will do", "i promise"]) {
                    0.9
                } else {
                    0.1
                }
            }
            "requests_action" => {
                if Self::any(&t, &["please", "can you", "could you", "would you"]) {
                    0.88
                } else {
                    0.1
                }
            }
            "security_alert" => {
                if Self::any(
                    &t,
                    &[
                        "security alert",
                        "sign-in",
                        "sign in from",
                        "password",
                        "suspicious",
                        "verify your account",
                        "2fa",
                        "two-factor",
                    ],
                ) {
                    0.93
                } else {
                    0.04
                }
            }
            "account_notification" => {
                if Self::any(&t, &["your account", "your plan", "your subscription"]) {
                    0.8
                } else {
                    0.1
                }
            }
            "social_notification" => {
                if Self::any(
                    &t,
                    &[
                        "liked",
                        "followed you",
                        "mentioned you",
                        "commented on",
                        "new follower",
                    ],
                ) {
                    0.9
                } else {
                    0.05
                }
            }
            "receipt" => {
                if Self::any(
                    &t,
                    &[
                        "receipt",
                        "thank you for your order",
                        "order confirmation",
                        "payment received",
                        "your order has",
                    ],
                ) {
                    0.92
                } else {
                    0.05
                }
            }
            "invoice" => {
                if Self::any(
                    &t,
                    &[
                        "invoice",
                        "amount due",
                        "payment due",
                        "due date",
                        "pay now",
                    ],
                ) {
                    0.9
                } else {
                    0.05
                }
            }
            "represents_charge" => {
                if Self::any(&t, &["charged", "was charged", "$", "€", "£"]) {
                    0.8
                } else {
                    0.1
                }
            }
            "payment_related" => {
                if Self::any(
                    &t,
                    &[
                        "invoice",
                        "payment",
                        "charged",
                        "$",
                        "amount due",
                        "wire",
                        "transfer",
                    ],
                ) {
                    0.88
                } else {
                    0.06
                }
            }
            "meeting_related" => {
                if Self::any(
                    &t,
                    &[
                        "meeting",
                        "invitation",
                        "calendar",
                        "invite",
                        "call at",
                        "standup",
                        "rescheduled",
                    ],
                ) {
                    0.9
                } else {
                    0.06
                }
            }
            "transactional" => {
                if Self::any(
                    &t,
                    &["order", "shipped", "confirmation", "receipt", "delivered"],
                ) {
                    0.85
                } else {
                    0.15
                }
            }
            "finance_topic" => {
                if Self::any(
                    &t,
                    &[
                        "tax",
                        "accountant",
                        "accounting",
                        "bookkeep",
                        "payroll",
                        "benefits",
                        "invest",
                        "insurance",
                        "corporation",
                        "rrsp",
                    ],
                ) {
                    0.9
                } else {
                    0.05
                }
            }
            "donation_appeal" => {
                if Self::any(
                    &t,
                    &[
                        "donate",
                        "donation",
                        "fundrais",
                        "your gift",
                        "charity",
                        "foundation",
                    ],
                ) {
                    0.9
                } else {
                    0.05
                }
            }
            "claims_prior_contact" => {
                if Self::any(
                    &t,
                    &[
                        "your comment",
                        "noticed your",
                        "saw your",
                        "we met",
                        "we spoke",
                        "as discussed",
                        "following up",
                    ],
                ) {
                    0.9
                } else {
                    0.08
                }
            }
            "travel_reservation" => {
                if Self::any(
                    &t,
                    &[
                        "flight",
                        "hotel",
                        "reservation",
                        "booking",
                        "itinerary",
                        "boarding",
                    ],
                ) {
                    0.92
                } else {
                    0.04
                }
            }
            "acknowledges_cancellation" => {
                if Self::any(&t, &["cancelled", "canceled", "cancellation"]) {
                    0.9
                } else {
                    0.05
                }
            }
            _ => 0.5,
        }
    }
}

#[async_trait]
impl SemanticJudge for ScriptedJudge {
    async fn evaluate(
        &self,
        state: &JudgeState,
        questions: &[JudgeQuestion],
    ) -> Result<Vec<JudgeAnswer>> {
        if self.unavailable.load(Ordering::SeqCst) {
            return Err(RiddleError::judge_unavailable(
                "scripted judge is unavailable",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_state.lock() = Some(state.text.clone());
        let overrides = self.overrides.lock().clone();
        let mut out = Vec::with_capacity(questions.len());
        for q in questions {
            let name = q.id.split('@').next().unwrap_or(&q.id).to_string();
            let mut distribution = Distribution::new();
            let selected = match q.primitive {
                SignalPrimitive::Noul => {
                    let yes = overrides
                        .get(&name)
                        .copied()
                        .unwrap_or_else(|| Self::yes_probability(&name, &state.text));
                    distribution.insert("YES".into(), yes);
                    distribution.insert("NO".into(), 1.0 - yes);
                    if yes >= 0.5 {
                        "YES".to_string()
                    } else {
                        "NO".to_string()
                    }
                }
                SignalPrimitive::Choice => {
                    let n = q.options.len().max(1) as f64;
                    let first = overrides.get(&name).copied().unwrap_or(0.6);
                    for (i, (opt, _)) in q.options.iter().enumerate() {
                        distribution.insert(
                            opt.clone(),
                            if i == 0 {
                                first
                            } else {
                                (1.0 - first) / (n - 1.0).max(1.0)
                            },
                        );
                    }
                    q.options
                        .first()
                        .map(|(o, _)| o.clone())
                        .unwrap_or_default()
                }
            };
            out.push(JudgeAnswer {
                question_id: q.id.clone(),
                selected,
                distribution,
                latency_ms: 1,
                input_tokens: state.approx_tokens as u64,
                deterministic: true,
                raw: Some(serde_json::json!({ "scripted": true })),
            });
        }
        Ok(out)
    }
    fn limits(&self) -> JudgeLimits {
        JudgeLimits {
            max_state_tokens: 32_000,
            max_request_tokens: 64_000,
            max_questions: 64,
            requests_per_minute: 1_200,
        }
    }
    fn identity(&self) -> JudgeIdentity {
        JudgeIdentity {
            provider: "scripted".into(),
            model_id: "scripted-1".into(),
            model_version: "1".into(),
            price_per_mtok: Some(0.042),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(name: &str) -> JudgeQuestion {
        JudgeQuestion {
            id: format!("{name}@v1"),
            primitive: SignalPrimitive::Noul,
            definition: name.into(),
            options: vec![("YES".into(), String::new()), ("NO".into(), String::new())],
        }
    }

    #[tokio::test]
    async fn the_scripted_judge_answers_from_keywords_and_honours_overrides() {
        let judge = ScriptedJudge::new().with_override("routine", 0.11);
        let state =
            JudgeState::new("Review requested: please review PR #4. Does it look right?".into());
        let answers = judge
            .evaluate(
                &state,
                &[q("review_requested"), q("asks_question"), q("routine")],
            )
            .await
            .unwrap();
        assert_eq!(answers[0].selected, "YES");
        assert!(answers[0].distribution["YES"] > 0.9);
        assert_eq!(answers[1].selected, "YES");
        assert!((answers[2].distribution["YES"] - 0.11).abs() < 1e-9);
        assert_eq!(judge.calls(), 1);
        judge.set_unavailable(true);
        assert_eq!(
            judge
                .evaluate(&state, &[q("routine")])
                .await
                .unwrap_err()
                .kind,
            riddle_errors::ErrorKind::JudgeUnavailable
        );
    }

    #[tokio::test]
    async fn the_null_judge_is_always_unavailable() {
        let err = NullJudge
            .evaluate(&JudgeState::new("x".into()), &[q("routine")])
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::JudgeUnavailable);
    }
}
