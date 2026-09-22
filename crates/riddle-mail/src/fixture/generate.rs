//! The synthetic mailbox. Deterministic from a seed so a test or a demo can
//! be re-run and a bug reproduced by seed alone. The populations mirror what
//! Sift must tell apart: review requests addressed to the user versus
//! routine PR chatter, CI failures on the user's repos versus others',
//! newsletters with `List-Unsubscribe`, receipts and invoices, calendar
//! parts, personal mail that asks something, the user's own asks that never
//! got a reply (Follow Up material), commitments the user made, mailing
//! lists, phishing lookalikes with failed authentication, automated alerts
//! and social notifications. Every message carries the headers Gmail would.

use crate::mime::new_message_id;
use chrono::{DateTime, Duration, Timelike, Utc};
use riddle_core::{
    Address, AttachmentMeta, AuthVerdict, AuthenticationResults, DigId, Message, MessageHeaders,
    MessageId, ThreadId,
};

/// SplitMix64: tiny, deterministic, no dependency version to drift.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// In `[lo, hi)`.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % (hi - lo) as u64) as i64
    }
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() % 10_000) as f64 / 10_000.0 < p
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next_u64() % items.len() as u64) as usize]
    }
}

#[derive(Debug, Clone)]
pub struct Persona {
    pub me: Address,
    pub work_domain: String,
    pub colleagues: Vec<Address>,
    pub boss: Address,
    pub personal: Vec<Address>,
    pub family: Vec<Address>,
    pub own_repos: Vec<String>,
    pub other_repos: Vec<String>,
}

impl Persona {
    pub fn for_user(email: &str, name: &str) -> Self {
        let work_domain = email
            .rsplit_once('@')
            .map(|(_, d)| d.to_string())
            .unwrap_or_else(|| "example.com".into());
        let at = |local: &str| format!("{local}@{work_domain}");
        Self {
            me: Address::new(email, Some(name)),
            colleagues: vec![
                Address::new(at("priya"), Some("Priya Natarajan")),
                Address::new(at("dan"), Some("Dan Whitfield")),
                Address::new(at("sofia"), Some("Sofia Reyes")),
                Address::new(at("tom"), Some("Tom Becker")),
            ],
            boss: Address::new(at("elena"), Some("Elena Marsh")),
            personal: vec![
                Address::new("jamie.chen@gmail.example", Some("Jamie Chen")),
                Address::new("alex.morgan@outlook.example", Some("Alex Morgan")),
                Address::new("rob.dyer@fastmail.example", Some("Rob Dyer")),
            ],
            family: vec![
                Address::new("susan.hale@gmail.example", Some("Susan Hale")),
                Address::new("ben.hale@gmail.example", Some("Ben Hale")),
            ],
            own_repos: vec!["example/riddle".into(), "example/atlas".into()],
            other_repos: vec![
                "example/billing-service".into(),
                "acme/widgets".into(),
                "example/infra".into(),
            ],
            work_domain,
        }
    }
}

/// The templates `inject` accepts and the generator draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    GithubReview,
    GithubCiFailure,
    GithubComment,
    Newsletter,
    Receipt,
    Invoice,
    Calendar,
    PersonalQuestion,
    ThreadReplied,
    SentAskUnanswered,
    SentCommitment,
    MailingList,
    Phishing,
    Alert,
    Social,
    Custom,
}

impl Template {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "github_review" => Template::GithubReview,
            "github_ci_failure" => Template::GithubCiFailure,
            "github_comment" => Template::GithubComment,
            "newsletter" => Template::Newsletter,
            "receipt" => Template::Receipt,
            "invoice" => Template::Invoice,
            "calendar" => Template::Calendar,
            "personal_question" => Template::PersonalQuestion,
            "thread_replied" => Template::ThreadReplied,
            "sent_ask" | "sent_ask_unanswered" => Template::SentAskUnanswered,
            "sent_commitment" => Template::SentCommitment,
            "mailing_list" => Template::MailingList,
            "phishing" => Template::Phishing,
            "alert" => Template::Alert,
            "social" => Template::Social,
            "custom" => Template::Custom,
            _ => return None,
        })
    }
    pub const NAMES: &'static [&'static str] = &[
        "github_review",
        "github_ci_failure",
        "github_comment",
        "newsletter",
        "receipt",
        "invoice",
        "calendar",
        "personal_question",
        "thread_replied",
        "sent_ask_unanswered",
        "sent_commitment",
        "mailing_list",
        "phishing",
        "alert",
        "social",
        "custom",
    ];
    /// Relative frequency in a generated mailbox.
    fn weight(self) -> u32 {
        match self {
            Template::GithubReview => 12,
            Template::GithubCiFailure => 20,
            Template::GithubComment => 45,
            Template::Newsletter => 50,
            Template::Receipt => 18,
            Template::Invoice => 8,
            Template::Calendar => 14,
            Template::PersonalQuestion => 24,
            Template::ThreadReplied => 18,
            Template::SentAskUnanswered => 10,
            Template::SentCommitment => 8,
            Template::MailingList => 30,
            Template::Phishing => 3,
            Template::Alert => 12,
            Template::Social => 20,
            Template::Custom => 0,
        }
    }
    const GENERATED: [Template; 15] = [
        Template::GithubReview,
        Template::GithubCiFailure,
        Template::GithubComment,
        Template::Newsletter,
        Template::Receipt,
        Template::Invoice,
        Template::Calendar,
        Template::PersonalQuestion,
        Template::ThreadReplied,
        Template::SentAskUnanswered,
        Template::SentCommitment,
        Template::MailingList,
        Template::Phishing,
        Template::Alert,
        Template::Social,
    ];
}

/// Overrides for an injected message.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub from: Option<Address>,
    pub subject: Option<String>,
    pub body: Option<String>,
}

pub struct Generator<'a> {
    pub dig: DigId,
    pub persona: &'a Persona,
    pub rng: Rng,
    pub now: DateTime<Utc>,
    pub next_id: u64,
}

fn auth(pass: bool) -> AuthenticationResults {
    let v = if pass {
        AuthVerdict::Pass
    } else {
        AuthVerdict::Fail
    };
    AuthenticationResults {
        spf: v,
        dkim: if pass {
            AuthVerdict::Pass
        } else {
            AuthVerdict::None
        },
        dmarc: v,
    }
}

impl<'a> Generator<'a> {
    pub fn new(
        dig: DigId,
        persona: &'a Persona,
        seed: u64,
        now: DateTime<Utc>,
        first_id: u64,
    ) -> Self {
        Self {
            dig,
            persona,
            rng: Rng::new(seed),
            now,
            next_id: first_id,
        }
    }

    fn id(&mut self) -> String {
        let id = format!("fx{:06x}", self.next_id);
        self.next_id += 1;
        id
    }

    /// A moment in the last `days` days, biased to working hours.
    fn when(&mut self, days: i64) -> DateTime<Utc> {
        let minutes_back = self.rng.range(60, days * 24 * 60);
        let t = self.now - Duration::minutes(minutes_back);
        let hour = if self.rng.chance(0.8) {
            self.rng.range(8, 20)
        } else {
            self.rng.range(0, 24)
        } as u32;
        t.with_hour(hour)
            .unwrap_or(t)
            .with_minute(self.rng.range(0, 60) as u32)
            .unwrap_or(t)
    }

    fn later(&mut self, after: DateTime<Utc>, min_h: i64, max_h: i64) -> DateTime<Utc> {
        let t = after + Duration::minutes(self.rng.range(min_h * 60, max_h * 60));
        if t > self.now {
            self.now - Duration::minutes(self.rng.range(5, 90))
        } else {
            t
        }
    }

    pub fn blank(&mut self, thread: Option<&ThreadId>, date: DateTime<Utc>) -> Message {
        let pid = self.id();
        let thread_id = thread
            .cloned()
            .unwrap_or_else(|| ThreadId::for_provider(&self.dig, &format!("t{}", &pid[2..])));
        Message {
            id: MessageId::for_provider(&self.dig, &pid),
            dig_id: self.dig.clone(),
            thread_id,
            provider_id: pid,
            subject: String::new(),
            from: Address::default(),
            to: vec![self.persona.me.clone()],
            cc: vec![],
            bcc: vec![],
            date,
            snippet: String::new(),
            labels: vec!["INBOX".into(), "UNREAD".into()],
            headers: MessageHeaders {
                message_id: Some(new_message_id("example.com")),
                authentication: auth(true),
                ..Default::default()
            },
            attachments: vec![],
            size_bytes: 0,
            is_sent: false,
            has_body: true,
            body_text: None,
            body_html: None,
            synced_at: self.now,
        }
    }

    fn finish(m: &mut Message, text: &str, html: Option<String>) {
        m.body_text = Some(text.to_string());
        m.body_html = html;
        m.snippet = crate::normalize::collapse_whitespace(text)
            .chars()
            .take(120)
            .collect();
        m.size_bytes =
            (text.len() + m.body_html.as_ref().map(|h| h.len()).unwrap_or(0) + 900) as u64;
        m.has_body = true;
        if m.headers.message_id.is_none() {
            m.headers.message_id = Some(new_message_id(&m.from.email));
        }
    }

    /// A reply from `from` in the same thread, threaded by headers.
    fn reply(
        &mut self,
        parent: &Message,
        from: &Address,
        to: Vec<Address>,
        text: &str,
        date: DateTime<Utc>,
    ) -> Message {
        let mut m = self.blank(Some(&parent.thread_id), date);
        m.from = from.clone();
        m.to = to;
        m.subject = if parent.subject.starts_with("Re: ") {
            parent.subject.clone()
        } else {
            format!("Re: {}", parent.subject)
        };
        m.headers.in_reply_to = parent.headers.message_id.clone();
        m.headers.references = parent
            .headers
            .references
            .iter()
            .cloned()
            .chain(parent.headers.message_id.clone())
            .collect();
        let quoted = format!(
            "{text}\n\nOn {} {} <{}> wrote:\n{}",
            parent.date.format("%a, %b %-d, %Y at %-I:%M %p"),
            parent.from.name.clone().unwrap_or_default(),
            parent.from.email,
            parent
                .body_text
                .clone()
                .unwrap_or_default()
                .lines()
                .map(|l| format!("> {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        if from.email == self.persona.me.email {
            m.is_sent = true;
            m.labels = vec!["SENT".into()];
            m.headers.authentication = auth(true);
        } else {
            m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_PERSONAL".into()];
        }
        Self::finish(&mut m, &quoted, None);
        m
    }

    /// Generate a whole mailbox of about `count` messages over `days` days.
    pub fn mailbox(&mut self, count: u32, days: i64) -> Vec<Message> {
        let mut out: Vec<Message> = Vec::with_capacity(count as usize + 8);
        // Every population appears at least once, then the weights decide.
        for t in Template::GENERATED {
            out.extend(self.scenario(t, days, &Overrides::default()));
        }
        let total: u32 = Template::GENERATED.iter().map(|t| t.weight()).sum();
        while (out.len() as u32) < count {
            let mut roll = self.rng.range(0, total as i64) as u32;
            let mut chosen = Template::Newsletter;
            for t in Template::GENERATED {
                if roll < t.weight() {
                    chosen = t;
                    break;
                }
                roll -= t.weight();
            }
            out.extend(self.scenario(chosen, days, &Overrides::default()));
        }
        out.sort_by_key(|m| m.date);
        out
    }

    /// One scenario; threads return several messages.
    pub fn scenario(&mut self, t: Template, days: i64, o: &Overrides) -> Vec<Message> {
        match t {
            Template::GithubReview => vec![self.github_review(days, o)],
            Template::GithubCiFailure => vec![self.github_ci_failure(days, o)],
            Template::GithubComment => self.github_comments(days, o),
            Template::Newsletter => vec![self.newsletter(days, o)],
            Template::Receipt => vec![self.receipt(days, o)],
            Template::Invoice => vec![self.invoice(days, o)],
            Template::Calendar => vec![self.calendar(days, o)],
            Template::PersonalQuestion => vec![self.personal_question(days, o)],
            Template::ThreadReplied => self.thread_replied(days, o),
            Template::SentAskUnanswered => vec![self.sent_ask(days, o)],
            Template::SentCommitment => self.sent_commitment(days, o),
            Template::MailingList => vec![self.mailing_list(days, o)],
            Template::Phishing => vec![self.phishing(days, o)],
            Template::Alert => vec![self.alert(days, o)],
            Template::Social => vec![self.social(days, o)],
            Template::Custom => vec![self.custom(days, o)],
        }
    }

    fn github_headers(m: &mut Message, repo: &str, reason: &str) {
        let list = repo.replace('/', ".");
        m.headers.list_id = Some(format!("{repo} <{list}.github.com>"));
        m.headers.precedence = Some("list".into());
        m.headers.list_unsubscribe = Some("<mailto:unsub+github@noreply.github.com>".into());
        m.headers.reply_to = Some(Address::new(
            format!("reply+{}@reply.github.com", &m.provider_id),
            Some(repo),
        ));
        m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_UPDATES".into()];
        m.cc = vec![Address::new(format!("{reason}@noreply.github.com"), None)];
    }

    fn github_review(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let author = self.rng.pick(&self.persona.colleagues).clone();
        let repo = self.rng.pick(&self.persona.own_repos).clone();
        let pr = self.rng.range(100, 400);
        let title = self
            .rng
            .pick(&[
                "Add outbox processor",
                "Tighten sanitizer allowlist",
                "Fix history cursor expiry",
                "Batch label modifications",
                "Thread state recompute on sent mail",
            ])
            .to_string();
        m.from = Address::new("notifications@github.com", author.name.as_deref());
        m.subject = o
            .subject
            .clone()
            .unwrap_or_else(|| format!("[{repo}] {title} (PR #{pr})"));
        Self::github_headers(&mut m, &repo, "review_requested");
        m.labels.push("IMPORTANT".into());
        let handle = author.email.split('@').next().unwrap_or("someone");
        let text = o.body.clone().unwrap_or_else(|| format!(
            "@{handle} requested your review on: #{pr} {title}.\n\nhttps://github.com/{repo}/pull/{pr}\n\n-- \nReply to this email directly, view it on GitHub, or unsubscribe.\nYou are receiving this because your review was requested."
        ));
        if let Some(f) = &o.from {
            m.from = f.clone();
        }
        Self::finish(&mut m, &text, Some(format!("<p>{}</p><p><a href=\"https://github.com/{repo}/pull/{pr}\">View pull request</a></p>", text.lines().next().unwrap_or(""))));
        m
    }

    fn github_ci_failure(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let own = self.rng.chance(0.6);
        let repo = if own {
            self.rng.pick(&self.persona.own_repos).clone()
        } else {
            self.rng.pick(&self.persona.other_repos).clone()
        };
        let sha = format!("{:07x}", self.rng.next_u64() & 0xFFF_FFFF);
        let workflow = self
            .rng
            .pick(&["CI", "Release", "Nightly", "Lint"])
            .to_string();
        m.from = Address::new("noreply@github.com", Some("GitHub"));
        m.subject = o
            .subject
            .clone()
            .unwrap_or_else(|| format!("[{repo}] Run failed: {workflow} - main ({sha})"));
        Self::github_headers(
            &mut m,
            &repo,
            if own { "ci_activity" } else { "subscribed" },
        );
        let reason = if own {
            "You are receiving this because you are the owner of this repository."
        } else {
            "You are receiving this because you are subscribed to this repository."
        };
        let text = o.body.clone().unwrap_or_else(|| format!(
            "Run failed for main ({sha})\n\nWorkflow: {workflow}\nRepository: {repo}\nFailed job: test (ubuntu-latest)\n\nView workflow run: https://github.com/{repo}/actions/runs/{}\n\n-- \n{reason}",
            self.rng.range(10_000_000, 99_999_999)
        ));
        if let Some(f) = &o.from {
            m.from = f.clone();
        }
        Self::finish(&mut m, &text, None);
        m
    }

    fn github_comments(&mut self, days: i64, o: &Overrides) -> Vec<Message> {
        let date = self.when(days);
        let repo = if self.rng.chance(0.4) {
            self.rng.pick(&self.persona.own_repos).clone()
        } else {
            self.rng.pick(&self.persona.other_repos).clone()
        };
        let pr = self.rng.range(20, 400);
        let title = self
            .rng
            .pick(&[
                "Fix rounding in totals",
                "Bump dependencies",
                "Refactor config loading",
                "Improve error messages",
                "Add retry to uploader",
            ])
            .to_string();
        let n = self.rng.range(1, 4) as usize;
        let mut out = Vec::new();
        let mut parent: Option<Message> = None;
        for i in 0..n {
            let author = self.rng.pick(&self.persona.colleagues).clone();
            let handle = author
                .email
                .split('@')
                .next()
                .unwrap_or("someone")
                .to_string();
            let comment = self
                .rng
                .pick(&[
                    "LGTM, one nit inline.",
                    "Can we add a test for the empty case?",
                    "Rebased on main.",
                    "Approved.",
                    "Looks like this needs a changelog entry.",
                ])
                .to_string();
            let when = match &parent {
                Some(p) => self.later(p.date, 1, 30),
                None => date,
            };
            let mut m = self.blank(parent.as_ref().map(|p| &p.thread_id), when);
            m.from = Address::new("notifications@github.com", author.name.as_deref());
            m.subject = o.subject.clone().unwrap_or_else(|| {
                format!(
                    "{}[{repo}] {title} (PR #{pr})",
                    if i == 0 { "" } else { "Re: " }
                )
            });
            Self::github_headers(&mut m, &repo, "subscribed");
            if let Some(p) = &parent {
                m.headers.in_reply_to = p.headers.message_id.clone();
                m.headers.references = vec![p.headers.message_id.clone().unwrap_or_default()];
            }
            let text = o.body.clone().unwrap_or_else(|| format!("@{handle} commented on this pull request.\n\n{comment}\n\n-- \nReply to this email directly, view it on GitHub, or unsubscribe.\nYou are receiving this because you are subscribed to this thread."));
            if let Some(f) = &o.from {
                m.from = f.clone();
            }
            Self::finish(&mut m, &text, None);
            if parent.is_none() {
                parent = Some(m.clone());
            }
            out.push(m);
        }
        out
    }

    fn newsletter(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let (sender, name, subject, category) = *self.rng.pick(&[
            (
                "newsletter@pragmaticengineer.example",
                "The Pragmatic Engineer",
                "The Pulse: what changed in platform teams this week",
                "CATEGORY_UPDATES",
            ),
            (
                "hello@morningbrew.example",
                "Morning Brew",
                "☕ Rates, rockets and a very expensive sandwich",
                "CATEGORY_PROMOTIONS",
            ),
            (
                "deals@outdoorgear.example",
                "Outdoor Gear Co",
                "Last chance: 40% off tents ends tonight",
                "CATEGORY_PROMOTIONS",
            ),
            (
                "digest@rustweekly.example",
                "This Week in Rust",
                "This Week in Rust 612",
                "CATEGORY_UPDATES",
            ),
            (
                "team@notion.example",
                "Notion",
                "New in Notion: databases that talk back",
                "CATEGORY_PROMOTIONS",
            ),
            (
                "no-reply@medium.example",
                "Medium Daily Digest",
                "Stories for you: why your on-call rotation is broken",
                "CATEGORY_UPDATES",
            ),
        ]);
        m.from = o
            .from
            .clone()
            .unwrap_or_else(|| Address::new(sender, Some(name)));
        m.subject = o.subject.clone().unwrap_or_else(|| subject.to_string());
        m.headers.list_id = Some(format!(
            "{name} <{}.list-id.example>",
            sender.split('@').next().unwrap_or("list")
        ));
        m.headers.list_unsubscribe = Some(format!(
            "<mailto:unsubscribe@{}>, <https://{}/unsubscribe?u={}>",
            m.from.domain(),
            m.from.domain(),
            self.rng.next_u64() % 100_000
        ));
        m.headers.precedence = Some("bulk".into());
        m.headers.authentication = AuthenticationResults {
            spf: AuthVerdict::Pass,
            dkim: AuthVerdict::Pass,
            dmarc: if self.rng.chance(0.7) {
                AuthVerdict::Pass
            } else {
                AuthVerdict::None
            },
        };
        m.labels = vec!["INBOX".into(), "UNREAD".into(), category.into()];
        let text = o.body.clone().unwrap_or_else(|| format!("{subject}\n\nThis week's highlights, picked for you.\n\n1. A long read on the thing everyone is talking about.\n2. A tool you did not know you needed.\n3. Jobs, events and a discount code.\n\nRead online: https://{}/issue/{}\n\nYou received this because you subscribed. Unsubscribe: https://{}/unsubscribe", m.from.domain(), self.rng.range(100, 999), m.from.domain()));
        let html = format!(
            "<html><body style=\"font-family:sans-serif\"><img src=\"https://{d}/track/open.gif?id={id}\" width=\"1\" height=\"1\"><h1>{subject}</h1><p>{first}</p><img src=\"https://{d}/images/hero.jpg\" alt=\"hero\"><p><a href=\"https://{d}/issue\">Read online</a> · <a href=\"https://{d}/unsubscribe\">Unsubscribe</a></p></body></html>",
            d = m.from.domain(),
            id = self.rng.next_u64() % 1_000_000,
            first = text.lines().nth(2).unwrap_or("")
        );
        Self::finish(&mut m, &text, Some(html));
        m
    }

    fn receipt(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let apple = self.rng.chance(0.35);
        let order = format!(
            "{}-{}-{}",
            self.rng.range(100, 999),
            self.rng.range(1_000_000, 9_999_999),
            self.rng.range(1_000_000, 9_999_999)
        );
        let item = self
            .rng
            .pick(&[
                "USB-C Hub (7-in-1)",
                "Mechanical keyboard switches",
                "Standing desk mat",
                "Noise-cancelling earbuds",
                "Trail running shoes",
            ])
            .to_string();
        let total = format!("${}.{:02}", self.rng.range(9, 240), self.rng.range(0, 99));
        if apple {
            m.from = Address::new("no_reply@email.apple.example", Some("Apple"));
            m.subject = o
                .subject
                .clone()
                .unwrap_or_else(|| "Your receipt from Apple.".into());
        } else {
            m.from = Address::new("auto-confirm@amazon.example", Some("Amazon.example"));
            m.subject = o
                .subject
                .clone()
                .unwrap_or_else(|| format!("Your Amazon.example order #{order} of \"{item}\""));
        }
        if let Some(f) = &o.from {
            m.from = f.clone();
        }
        m.headers.auto_submitted = Some("auto-generated".into());
        m.labels = vec!["INBOX".into(), "CATEGORY_UPDATES".into()];
        let text = o.body.clone().unwrap_or_else(|| if apple {
            format!("Receipt\n\nApple ID: {}\nOrder ID: ML{}\n\n{} — Monthly subscription\nTotal: {total}\n\nThank you for your purchase.", self.persona.me.email, self.rng.range(1_000_000, 9_999_999), self.rng.pick(&["iCloud+ 200GB", "Apple Music", "Things 3 for Mac", "Fantastical Premium"]))
        } else {
            format!("Order Confirmation\n\nHello {},\n\nThank you for your order. We'll send a confirmation when your item ships.\n\nOrder number: {order}\nItem: {item}\nOrder total: {total}\n\nArriving: {}", self.persona.me.name.clone().unwrap_or_default(), (date + Duration::days(3)).format("%A, %B %-d"))
        });
        let html = format!(
            "<html><body><div itemscope itemtype=\"http://schema.org/Order\"><span itemprop=\"orderNumber\">{order}</span><span itemprop=\"price\">{total}</span></div><p>{}</p><img src=\"https://{}/img/logo.png\"></body></html>",
            text.replace('\n', "<br>"),
            m.from.domain()
        );
        Self::finish(&mut m, &text, Some(html));
        m
    }

    fn invoice(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let (sender, name) = *self.rng.pick(&[
            ("billing@northwind.example", "Northwind Consulting"),
            ("invoices@cloudhost.example", "CloudHost Billing"),
            ("accounts@studiotype.example", "Studio Type"),
        ]);
        let number = format!("{}-{:04}", date.format("%Y"), self.rng.range(1, 9999));
        let amount = format!(
            "${},{:03}.00",
            self.rng.range(1, 12),
            self.rng.range(0, 999)
        );
        m.from = o
            .from
            .clone()
            .unwrap_or_else(|| Address::new(sender, Some(name)));
        m.subject = o
            .subject
            .clone()
            .unwrap_or_else(|| format!("Invoice #{number} from {name}"));
        m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_UPDATES".into()];
        m.attachments = vec![AttachmentMeta {
            filename: format!("invoice-{number}.pdf"),
            mime_type: "application/pdf".into(),
            size_bytes: 48_211,
            provider_id: Some(format!("att-{}", m.provider_id)),
            content_id: None,
        }];
        let due = (date + Duration::days(30)).format("%B %-d, %Y");
        let text = o.body.clone().unwrap_or_else(|| format!("Hi {},\n\nPlease find attached invoice #{number} for services in {}. Amount due: {amount}. Payment is due by {due}.\n\nPay online: https://{}/pay/{number}\n\nThanks,\n{name} Accounts", self.persona.me.name.clone().unwrap_or_default(), (date - Duration::days(20)).format("%B"), m.from.domain()));
        Self::finish(&mut m, &text, None);
        m
    }

    fn calendar(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let organiser = if self.rng.chance(0.7) {
            self.rng.pick(&self.persona.colleagues).clone()
        } else {
            self.persona.boss.clone()
        };
        let title = self
            .rng
            .pick(&[
                "Sprint review",
                "1:1",
                "Architecture sync",
                "Q4 planning",
                "Vendor demo: observability",
            ])
            .to_string();
        let start = self.later(date, 24, 24 * 10);
        m.from = o
            .from
            .clone()
            .unwrap_or_else(|| Address::new(organiser.email.clone(), organiser.name.as_deref()));
        m.subject = o.subject.clone().unwrap_or_else(|| {
            format!(
                "Invitation: {title} @ {} ({})",
                start.format("%a %b %-d, %Y %-I%P - %-I%P"),
                self.persona.me.email
            )
        });
        m.labels = vec!["INBOX".into(), "UNREAD".into()];
        m.headers.reply_to = Some(organiser.clone());
        let ics = format!("BEGIN:VCALENDAR\nMETHOD:REQUEST\nBEGIN:VEVENT\nDTSTART:{}\nSUMMARY:{title}\nORGANIZER:mailto:{}\nATTENDEE:mailto:{}\nEND:VEVENT\nEND:VCALENDAR", start.format("%Y%m%dT%H%M%SZ"), organiser.email, self.persona.me.email);
        m.attachments = vec![AttachmentMeta {
            filename: "invite.ics".into(),
            mime_type: "text/calendar".into(),
            size_bytes: ics.len() as u64,
            provider_id: Some(format!("att-{}", m.provider_id)),
            content_id: None,
        }];
        let text = o.body.clone().unwrap_or_else(|| format!("You have been invited to the following event.\n\n{title}\nWhen: {}\nOrganizer: {}\n\nJoining? Yes - Maybe - No", start.format("%A %B %-d, %Y %-I:%M%P"), organiser.name.clone().unwrap_or_default()));
        Self::finish(
            &mut m,
            &text,
            Some(format!("<p>{}</p>", text.replace('\n', "<br>"))),
        );
        m
    }

    fn personal_question(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let roll = self.rng.range(0, 3);
        let (from, text): (Address, String) = match roll {
            0 => {
                let f = self.rng.pick(&self.persona.family).clone();
                let q = self
                    .rng
                    .pick(&[
                        "Are you coming for dinner on Sunday? Ben is bringing the kids.",
                        "Did you get the photos I sent? Which one should we frame?",
                        "Can you call me this week about Dad's birthday plans?",
                    ])
                    .to_string();
                (f, format!("Hi love,\n\n{q}\n\nMum x"))
            }
            1 => {
                let f = self.rng.pick(&self.persona.personal).clone();
                let q = self
                    .rng
                    .pick(&[
                        "Are you free for a run Saturday morning? Thinking the coastal loop.",
                        "Did you decide about the cabin weekend? I need to confirm by Friday.",
                        "Could you send me that pasta recipe you mentioned?",
                        "Any chance you could look over my CV before I send it Thursday?",
                    ])
                    .to_string();
                (
                    f.clone(),
                    format!(
                        "Hey,\n\n{q}\n\nCheers,\n{}",
                        f.name
                            .clone()
                            .unwrap_or_default()
                            .split(' ')
                            .next()
                            .unwrap_or("")
                    ),
                )
            }
            _ => {
                let f = if self.rng.chance(0.3) {
                    self.persona.boss.clone()
                } else {
                    self.rng.pick(&self.persona.colleagues).clone()
                };
                let q = self.rng.pick(&["Can you send me the Q3 numbers before Thursday's review?", "Would you have time to look at the outage write-up today?", "Do you want to present the sync design at the platform meeting, or should I?", "Could you approve the vendor contract in the portal when you get a chance?"]).to_string();
                (
                    f.clone(),
                    format!(
                        "Hi {},\n\n{q}\n\nThanks,\n{}",
                        self.persona.me.name.clone().unwrap_or_default(),
                        f.name
                            .clone()
                            .unwrap_or_default()
                            .split(' ')
                            .next()
                            .unwrap_or("")
                    ),
                )
            }
        };
        m.from = o.from.clone().unwrap_or(from);
        m.subject = o.subject.clone().unwrap_or_else(|| {
            self.rng
                .pick(&[
                    "Quick question",
                    "This week",
                    "Sunday",
                    "Numbers",
                    "Catching up",
                ])
                .to_string()
        });
        m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_PERSONAL".into()];
        if roll == 2 && self.rng.chance(0.5) {
            m.labels.push("IMPORTANT".into());
        }
        let text = o.body.clone().unwrap_or(text);
        Self::finish(&mut m, &text, None);
        m
    }

    fn thread_replied(&mut self, days: i64, o: &Overrides) -> Vec<Message> {
        let date = self.when(days);
        let them = if self.rng.chance(0.6) {
            self.rng.pick(&self.persona.colleagues).clone()
        } else {
            self.rng.pick(&self.persona.personal).clone()
        };
        let mut first = self.blank(None, date);
        first.from = o.from.clone().unwrap_or_else(|| them.clone());
        first.subject = o.subject.clone().unwrap_or_else(|| {
            self.rng
                .pick(&[
                    "Deploy window",
                    "Offsite dates",
                    "Draft agenda",
                    "Reference for Priya",
                    "Weekend plans",
                ])
                .to_string()
        });
        first.labels = vec!["INBOX".into(), "CATEGORY_PERSONAL".into()];
        let ask = self
            .rng
            .pick(&[
                "Does Tuesday evening work for the deploy window?",
                "Which offsite dates suit you, the 12th or the 19th?",
                "Can you add your items to the agenda by tomorrow?",
                "Would you be a reference for me?",
            ])
            .to_string();
        let text = o.body.clone().unwrap_or_else(|| {
            format!(
                "Hi {},\n\n{ask}\n\n{}",
                self.persona.me.name.clone().unwrap_or_default(),
                them.name.clone().unwrap_or_default()
            )
        });
        Self::finish(&mut first, &text, None);
        let me = self.persona.me.clone();
        let reply_when = self.later(first.date, 1, 24);
        let answer = self
            .rng
            .pick(&[
                "Tuesday works for me.",
                "The 19th is better for me.",
                "Done, added three items.",
                "Of course, happy to.",
            ])
            .to_string();
        let mine = self.reply(&first, &me, vec![them.clone()], &answer, reply_when);
        let mut out = vec![first.clone(), mine.clone()];
        if self.rng.chance(0.5) {
            let when = self.later(mine.date, 1, 48);
            let thanks = self
                .rng
                .pick(&[
                    "Great, thanks!",
                    "Perfect, booked.",
                    "Thank you, appreciate it.",
                ])
                .to_string();
            let theirs = self.reply(&mine, &them, vec![me.clone()], &thanks, when);
            out.push(theirs);
        }
        out
    }

    fn sent_ask(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let to = self.rng.pick(&self.persona.colleagues).clone();
        m.from = self.persona.me.clone();
        m.to = vec![to.clone()];
        m.is_sent = true;
        m.labels = vec!["SENT".into()];
        m.subject = o.subject.clone().unwrap_or_else(|| {
            self.rng
                .pick(&[
                    "Board deck numbers",
                    "Access to the staging account",
                    "Vendor quote",
                    "Design doc review",
                ])
                .to_string()
        });
        let ask = self
            .rng
            .pick(&[
                "Can you send me the updated numbers for the board deck?",
                "Could you grant me access to the staging account?",
                "Can you forward the vendor quote when it arrives?",
                "Would you review the design doc by Wednesday?",
            ])
            .to_string();
        let text = o.body.clone().unwrap_or_else(|| {
            format!(
                "Hi {},\n\n{ask}\n\nThanks,\n{}",
                to.name
                    .clone()
                    .unwrap_or_default()
                    .split(' ')
                    .next()
                    .unwrap_or(""),
                self.persona.me.name.clone().unwrap_or_default()
            )
        });
        Self::finish(&mut m, &text, None);
        m
    }

    fn sent_commitment(&mut self, days: i64, o: &Overrides) -> Vec<Message> {
        let date = self.when(days);
        let them = if self.rng.chance(0.5) {
            self.persona.boss.clone()
        } else {
            self.rng.pick(&self.persona.colleagues).clone()
        };
        let mut first = self.blank(None, date);
        first.from = o.from.clone().unwrap_or_else(|| them.clone());
        first.subject = o.subject.clone().unwrap_or_else(|| {
            self.rng
                .pick(&["Numbers for Friday", "Migration plan", "Incident follow-up"])
                .to_string()
        });
        first.labels = vec!["INBOX".into(), "CATEGORY_PERSONAL".into()];
        let text = o.body.clone().unwrap_or_else(|| format!("Hi {},\n\nWhere are we on this? I need something before the end of the week.\n\n{}", self.persona.me.name.clone().unwrap_or_default(), them.name.clone().unwrap_or_default()));
        Self::finish(&mut first, &text, None);
        let me = self.persona.me.clone();
        let when = self.later(first.date, 1, 12);
        let commit = self
            .rng
            .pick(&[
                "I'll get you the numbers by Friday.",
                "I will send the migration plan by end of day tomorrow.",
                "I can have the incident write-up to you by Thursday.",
            ])
            .to_string();
        let mine = self.reply(&first, &me, vec![them], &commit, when);
        vec![first, mine]
    }

    fn mailing_list(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let (list, domain, topic) = *self.rng.pick(&[
            (
                "dev-list",
                "lists.example.com",
                "Proposal: move the monorepo to Bazel",
            ),
            (
                "platform-announce",
                "lists.example.com",
                "Maintenance window Saturday 02:00 UTC",
            ),
            (
                "sqlite-users",
                "sqlite.example",
                "WAL checkpoint starvation under heavy readers",
            ),
            (
                "rust-embedded",
                "lists.rust.example",
                "[ANN] embedded-hal 2.0 released",
            ),
        ]);
        let poster = if domain == "lists.example.com" {
            self.rng.pick(&self.persona.colleagues).clone()
        } else {
            Address::new(
                format!(
                    "poster{}@{}",
                    self.rng.range(1, 60),
                    self.rng
                        .pick(&["mailbox.example", "posteo.example", "hey.example"])
                ),
                Some(
                    self.rng
                        .pick(&["Mika Ito", "Lars Ekberg", "Ana Costa", "Jordan Blake"]),
                ),
            )
        };
        m.from = o.from.clone().unwrap_or(poster);
        m.to = vec![Address::new(format!("{list}@{domain}"), Some(list))];
        m.subject = o
            .subject
            .clone()
            .unwrap_or_else(|| format!("[{list}] {topic}"));
        m.headers.list_id = Some(format!("{list} <{list}.{domain}>"));
        m.headers.list_unsubscribe = Some(format!("<mailto:{list}-unsubscribe@{domain}>"));
        m.headers.precedence = Some("list".into());
        m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_FORUMS".into()];
        let text = o.body.clone().unwrap_or_else(|| format!("{topic}\n\nHas anyone else run into this? Details and a repro below.\n\n{}\n\n-- \n{list} mailing list\n{list}@{domain}\nTo unsubscribe send an email to {list}-unsubscribe@{domain}", self.rng.pick(&["Steps: 1) build 2) run 3) observe.", "We measured a 12% regression after the upgrade.", "Patch attached to the tracker issue."])));
        Self::finish(&mut m, &text, None);
        m
    }

    fn phishing(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let variant = self.rng.range(0, 3);
        let (from, subject, text) = match variant {
            0 => (
                Address::new("security@micros0ft-support.example", Some("Microsoft Account Team")),
                "Unusual sign-in activity — action required".to_string(),
                "We detected an unusual sign-in to your account from a new device.\n\nYour account will be suspended within 24 hours unless you verify your identity. Verify your password now:\nhttp://micros0ft-support.example/verify?u=matt\n\nMicrosoft Account Team".to_string(),
            ),
            1 => {
                let lookalike = self.persona.work_domain.replacen('l', "1", 1).replacen('e', "3", 1);
                (
                    Address::new(format!("helpdesk@{lookalike}"), Some("IT Helpdesk")),
                    "URGENT: Password expires today".to_string(),
                    format!("Your corporate password expires today. To avoid losing access, re-enter your password at the portal below immediately.\n\nhttps://{lookalike}/sso/login\n\nIT Helpdesk"),
                )
            }
            _ => (
                Address::new("elena.marsh.ceo@gmail.example", self.persona.boss.name.as_deref()),
                "Quick favour".to_string(),
                "Are you at your desk? I need you to buy 5 Apple gift cards for a client today and send me the codes. I'm in meetings and can't talk. Keep this between us for now.\n\nSent from my iPhone".to_string(),
            ),
        };
        m.from = o.from.clone().unwrap_or(from);
        m.subject = o.subject.clone().unwrap_or(subject);
        m.headers.reply_to = Some(Address::new(
            format!("reply{}@mail-relay.example", self.rng.range(10, 99)),
            None,
        ));
        m.headers.authentication = AuthenticationResults {
            spf: AuthVerdict::Fail,
            dkim: AuthVerdict::None,
            dmarc: AuthVerdict::Fail,
        };
        m.labels = if self.rng.chance(0.3) {
            vec!["SPAM".into(), "UNREAD".into()]
        } else {
            vec!["INBOX".into(), "UNREAD".into()]
        };
        let text = o.body.clone().unwrap_or(text);
        Self::finish(
            &mut m,
            &text,
            Some(format!(
                "<p>{}</p><img src=\"https://mail-relay.example/o/{}.gif\">",
                text.replace('\n', "<br>"),
                self.rng.next_u64() % 100_000
            )),
        );
        m
    }

    fn alert(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let (from, name, subject, body) = *self.rng.pick(&[
            ("no-reply-aws@amazon.example", "Amazon Web Services", "AWS Budgets: Monthly budget has exceeded 80% of the limit", "Your AWS budget 'monthly-prod' has reached 82% of its $1,200.00 limit for this period.\n\nView in the Billing console: https://console.aws.example/billing\n\nThis is an automated message."),
            ("no-reply@accounts.google.example", "Google", "Security alert", "A new sign-in on Linux\n\nWe noticed a new sign-in to your Google Account on a Linux device. If this was you, you don't need to do anything. If not, we'll help you secure your account.\n\nCheck activity: https://myaccount.google.example/notifications"),
            ("alerts@datadog.example", "Datadog", "[Triggered] p95 latency above 800ms on api-gateway", "Monitor: api-gateway p95 latency\nStatus: Triggered\nValue: 942ms (threshold 800ms)\n\nhttps://app.datadog.example/monitors/7712"),
            ("no-reply@pagerduty.example", "PagerDuty", "[Resolved] #4412 disk usage warning on db-2", "Incident #4412 was resolved automatically.\n\nService: db-2\nDuration: 14 minutes"),
        ]);
        m.from = o
            .from
            .clone()
            .unwrap_or_else(|| Address::new(from, Some(name)));
        m.subject = o.subject.clone().unwrap_or_else(|| subject.to_string());
        m.headers.auto_submitted = Some("auto-generated".into());
        m.headers.precedence = Some("bulk".into());
        m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_UPDATES".into()];
        let text = o.body.clone().unwrap_or_else(|| body.to_string());
        Self::finish(&mut m, &text, None);
        m
    }

    fn social(&mut self, days: i64, o: &Overrides) -> Message {
        let date = self.when(days);
        let mut m = self.blank(None, date);
        let who = self
            .rng
            .pick(&self.persona.colleagues)
            .name
            .clone()
            .unwrap_or_default();
        let (from, name, subject, body) = *self.rng.pick(&[
            (
                "messages-noreply@linkedin.example",
                "LinkedIn",
                "{who} and 3 others viewed your profile",
                "See who's looking at your profile this week.",
            ),
            (
                "notification@facebookmail.example",
                "Facebook",
                "{who} commented on your photo",
                "\"Love this!\" — see the comment and reply.",
            ),
            (
                "no-reply@strava.example",
                "Strava",
                "{who} gave you kudos",
                "Your morning run got 4 kudos.",
            ),
            (
                "noreply@slack.example",
                "Slack",
                "New activity in #platform",
                "You have 3 unread messages in #platform.",
            ),
        ]);
        m.from = o
            .from
            .clone()
            .unwrap_or_else(|| Address::new(from, Some(name)));
        m.subject = o
            .subject
            .clone()
            .unwrap_or_else(|| subject.replace("{who}", &who));
        m.headers.list_unsubscribe = Some(format!("<https://{}/unsubscribe>", m.from.domain()));
        m.headers.precedence = Some("bulk".into());
        m.labels = vec!["INBOX".into(), "CATEGORY_SOCIAL".into()];
        let domain = m.from.domain().to_string();
        let text = o.body.clone().unwrap_or_else(|| {
            format!(
                "{}\n\nhttps://{domain}/notifications\n\nUnsubscribe: https://{domain}/unsubscribe",
                body.replace("{who}", &who)
            )
        });
        let html = format!(
            "<p>{}</p><img src=\"https://{domain}/pixel.png\">",
            text.lines().next().unwrap_or("")
        );
        Self::finish(&mut m, &text, Some(html));
        m
    }

    fn custom(&mut self, _days: i64, o: &Overrides) -> Message {
        let date = self.now - Duration::seconds(self.rng.range(1, 30));
        let mut m = self.blank(None, date);
        m.from = o
            .from
            .clone()
            .unwrap_or_else(|| self.persona.personal[0].clone());
        m.subject = o
            .subject
            .clone()
            .unwrap_or_else(|| "Hello from Demo Mode".into());
        m.labels = vec!["INBOX".into(), "UNREAD".into(), "CATEGORY_PERSONAL".into()];
        let text = o
            .body
            .clone()
            .unwrap_or_else(|| "Just checking Riddle sees this.".into());
        Self::finish(&mut m, &text, None);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_produces_the_same_mailbox() {
        let dig = DigId::from("d1");
        let persona = Persona::for_user("matt@example.com", "Matt");
        let now = Utc::now();
        let a = Generator::new(dig.clone(), &persona, 7, now, 1).mailbox(120, 60);
        let b = Generator::new(dig, &persona, 7, now, 1).mailbox(120, 60);
        assert_eq!(a.len(), b.len());
        assert!(a.len() >= 120);
        assert!(a.iter().zip(&b).all(|(x, y)| x.provider_id == y.provider_id
            && x.subject == y.subject
            && x.date == y.date));
    }

    #[test]
    fn every_population_is_present_with_correct_headers() {
        let dig = DigId::from("d1");
        let persona = Persona::for_user("matt@example.com", "Matt");
        let ms = Generator::new(dig, &persona, 42, Utc::now(), 1).mailbox(400, 60);
        assert!(ms.iter().any(|m| m.subject.contains("(PR #")
            && m.cc
                .iter()
                .any(|c| c.email.starts_with("review_requested@"))));
        assert!(ms.iter().any(|m| m.subject.contains("Run failed")));
        assert!(ms.iter().any(|m| m.headers.list_unsubscribe.is_some()
            && m.labels.contains(&"CATEGORY_PROMOTIONS".to_string())));
        assert!(ms.iter().any(|m| m
            .body_html
            .as_deref()
            .is_some_and(|h| h.contains("schema.org/Order"))));
        assert!(ms
            .iter()
            .any(|m| m.attachments.iter().any(|a| a.mime_type == "text/calendar")));
        assert!(ms.iter().any(|m| m.is_sent
            && m.body_text.as_deref().is_some_and(|t| t.contains("Can you")
                || t.contains("Could you")
                || t.contains("Would you"))));
        assert!(ms.iter().any(|m| m.is_sent
            && m.body_text.as_deref().is_some_and(|t| t.contains("I'll")
                || t.contains("I will")
                || t.contains("I can"))));
        assert!(ms
            .iter()
            .any(|m| m.headers.authentication.spf == AuthVerdict::Fail));
        assert!(ms
            .iter()
            .any(|m| m.labels.contains(&"CATEGORY_SOCIAL".to_string())));
        assert!(ms
            .iter()
            .any(|m| m.labels.contains(&"CATEGORY_FORUMS".to_string())
                && m.headers.precedence.as_deref() == Some("list")));
        assert!(ms.windows(2).all(|w| w[0].date <= w[1].date));
        let ids: std::collections::HashSet<_> = ms.iter().map(|m| m.provider_id.clone()).collect();
        assert_eq!(ids.len(), ms.len());
    }
}
