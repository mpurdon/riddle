//! The Mesh text DSL, exactly as §2 shows it:
//!
//! ```text
//! Mesh: GitHub Notifications v8
//! Scope:
//! Work Digs
//! ROUTES
//! IF review_requested@v3 > .80 → Find: Review
//! ELSE IF ci_failure@v4 > .85 AND personally_relevant@v3 > .70 → Find: Do
//! ELSE → Escalate
//! GUARDS
//! NEVER Spoil IF sender IN never_hidden
//! ```
//!
//! Keywords are case-insensitive, both `→` and `->` are arrows, conditions
//! may span lines, an output runs to the end of its line and combines with
//! `+`. Every error carries the line number in `detail` so the editor can
//! point at it.

use riddle_core::{
    Autonomy, CmpOp, CreatedBy, Disposition, DispositionKind, Expiry, Expr, FactValue, FindType,
    Guard, Importance, Mesh, MeshId, MeshOutput, MeshScope, MeshStatus, NamedList, Route,
};
use riddle_errors::{Result, RiddleError};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedHeader {
    pub name: Option<String>,
    pub version: Option<u32>,
    pub scope: MeshScope,
    /// Whether the text carried a `Scope:`; without one an update keeps the
    /// previous version's scope instead of silently widening to All Digs.
    pub scope_given: bool,
    pub priority: i32,
    /// Whether the text carried a `Priority:`; without one a new Mesh takes
    /// the first free priority and an update keeps the previous version's.
    pub priority_given: bool,
    /// `Autonomy: Observe | Suggest | Act`: the most this Mesh may do in the
    /// mailbox. Absent means "as the account"; an update without the line
    /// keeps the previous version's cap.
    pub autonomy: Option<Autonomy>,
    pub autonomy_given: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Signal {
        name: String,
        version: u32,
        choice: Option<String>,
    },
    Number(f64),
    Str(String),
    Op(CmpOp),
    LParen,
    RParen,
    /// The arrow and the raw output text after it.
    Arrow(String),
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: usize,
}

fn err(line: usize, msg: impl Into<String>) -> RiddleError {
    let msg = msg.into();
    RiddleError::invalid(format!("line {line}: {msg}"))
        .with_detail(serde_json::json!({ "line": line, "problem": msg }))
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'
}

fn tokenize(lines: &[(usize, &str)]) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    for (line_no, raw) in lines {
        let line = *line_no;
        let text = raw.split('#').next().unwrap_or("");
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if c == '→' || (c == '-' && chars.get(i + 1) == Some(&'>')) {
                let skip = if c == '→' { 1 } else { 2 };
                let rest: String = chars[i + skip..].iter().collect();
                out.push(Token {
                    tok: Tok::Arrow(rest.trim().to_string()),
                    line,
                });
                i = chars.len();
                continue;
            }
            if c == '(' {
                out.push(Token {
                    tok: Tok::LParen,
                    line,
                });
                i += 1;
                continue;
            }
            if c == ')' {
                out.push(Token {
                    tok: Tok::RParen,
                    line,
                });
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                let quote = c;
                let mut j = i + 1;
                while j < chars.len() && chars[j] != quote {
                    j += 1;
                }
                if j >= chars.len() {
                    return Err(err(line, "unterminated string"));
                }
                out.push(Token {
                    tok: Tok::Str(chars[i + 1..j].iter().collect()),
                    line,
                });
                i = j + 1;
                continue;
            }
            if c == '>' || c == '<' || c == '=' || c == '!' {
                let two = chars.get(i + 1) == Some(&'=');
                let op = match (c, two) {
                    ('>', true) => CmpOp::Gte,
                    ('>', false) => CmpOp::Gt,
                    ('<', true) => CmpOp::Lte,
                    ('<', false) => CmpOp::Lt,
                    ('=', _) => CmpOp::Eq,
                    ('!', true) => CmpOp::Neq,
                    _ => return Err(err(line, "unexpected '!'")),
                };
                out.push(Token {
                    tok: Tok::Op(op),
                    line,
                });
                i += if two { 2 } else { 1 };
                continue;
            }
            if c.is_ascii_digit()
                || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
            {
                let mut j = i;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                    j += 1;
                }
                let s: String = chars[i..j].iter().collect();
                let n: f64 = s
                    .parse()
                    .map_err(|_| err(line, format!("bad number {s}")))?;
                out.push(Token {
                    tok: Tok::Number(n),
                    line,
                });
                i = j;
                continue;
            }
            if is_ident_start(c) {
                let mut j = i;
                while j < chars.len() && is_ident(chars[j]) {
                    j += 1;
                }
                let name: String = chars[i..j].iter().collect();
                if chars.get(j) == Some(&'@') {
                    // signal@vN[:CHOICE]
                    let mut k = j + 1;
                    if chars.get(k).map(|c| c.eq_ignore_ascii_case(&'v')) == Some(true) {
                        k += 1;
                    }
                    let start = k;
                    while k < chars.len() && chars[k].is_ascii_digit() {
                        k += 1;
                    }
                    if start == k {
                        return Err(err(
                            line,
                            format!("signal reference {name}@ needs a version like {name}@v3"),
                        ));
                    }
                    let version: u32 = chars[start..k]
                        .iter()
                        .collect::<String>()
                        .parse()
                        .map_err(|_| err(line, "bad signal version"))?;
                    let mut choice = None;
                    if chars.get(k) == Some(&':') {
                        let mut m = k + 1;
                        while m < chars.len()
                            && (chars[m].is_ascii_alphanumeric() || chars[m] == '_')
                        {
                            m += 1;
                        }
                        if m == k + 1 {
                            return Err(err(line, "signal choice after ':' is empty"));
                        }
                        choice = Some(chars[k + 1..m].iter().collect());
                        k = m;
                    }
                    out.push(Token {
                        tok: Tok::Signal {
                            name,
                            version,
                            choice,
                        },
                        line,
                    });
                    i = k;
                } else if name.eq_ignore_ascii_case("list") && chars.get(j) == Some(&':') {
                    // list:banks, a named list, read as one word.
                    let mut k = j + 1;
                    while k < chars.len() && is_ident(chars[k]) {
                        k += 1;
                    }
                    if k == j + 1 {
                        return Err(err(line, "list: needs a name, like list:banks"));
                    }
                    let list: String = chars[j + 1..k].iter().collect();
                    out.push(Token {
                        tok: Tok::Ident(format!("list:{}", list.to_ascii_lowercase())),
                        line,
                    });
                    i = k;
                } else {
                    out.push(Token {
                        tok: Tok::Ident(name),
                        line,
                    });
                    i = j;
                }
                continue;
            }
            return Err(err(line, format!("unexpected character {c:?}")));
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }
    fn line(&self) -> usize {
        self.peek()
            .map(|t| t.line)
            .or_else(|| self.toks.last().map(|t| t.line))
            .unwrap_or(0)
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Token { tok: Tok::Ident(s), .. }) if s.eq_ignore_ascii_case(kw))
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.is_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expr(&mut self) -> Result<Expr> {
        self.or()
    }
    fn or(&mut self) -> Result<Expr> {
        let mut items = vec![self.and()?];
        while self.eat_kw("OR") {
            items.push(self.and()?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Expr::Or { items }
        })
    }
    fn and(&mut self) -> Result<Expr> {
        let mut items = vec![self.unary()?];
        while self.eat_kw("AND") {
            items.push(self.unary()?);
        }
        Ok(if items.len() == 1 {
            items.pop().unwrap()
        } else {
            Expr::And { items }
        })
    }
    fn unary(&mut self) -> Result<Expr> {
        if self.eat_kw("NOT") {
            return Ok(Expr::Not {
                item: Box::new(self.unary()?),
            });
        }
        self.primary()
    }
    fn primary(&mut self) -> Result<Expr> {
        let line = self.line();
        let Some(t) = self.next() else {
            return Err(err(line, "expected a condition"));
        };
        match t.tok {
            Tok::LParen => {
                let e = self.expr()?;
                match self.next() {
                    Some(Token {
                        tok: Tok::RParen, ..
                    }) => Ok(e),
                    _ => Err(err(line, "missing ')'")),
                }
            }
            Tok::Signal {
                name,
                version,
                choice,
            } => {
                let op = match self.next() {
                    Some(Token {
                        tok: Tok::Op(op), ..
                    }) => op,
                    _ => {
                        return Err(err(
                            line,
                            format!("{name}@v{version} needs a comparison like > .80"),
                        ))
                    }
                };
                let threshold = match self.next() {
                    Some(Token {
                        tok: Tok::Number(n),
                        ..
                    }) => n,
                    _ => {
                        return Err(err(
                            line,
                            format!("{name}@v{version} {} needs a number", op.as_str()),
                        ))
                    }
                };
                if !(0.0..=1.0).contains(&threshold) {
                    return Err(err(
                        line,
                        format!("threshold {threshold} must be between 0 and 1"),
                    ));
                }
                Ok(Expr::Signal {
                    signal: name,
                    version,
                    choice,
                    op,
                    threshold,
                })
            }
            Tok::Ident(name) => {
                let mut lower = name.to_ascii_lowercase();
                if lower == "true" || lower == "false" {
                    return Err(err(line, "a bare true/false is meaningless here"));
                }
                // `header X-GitHub-Reason = ci_activity`
                if lower == "header" {
                    match self.next() {
                        Some(Token {
                            tok: Tok::Ident(h), ..
                        })
                        | Some(Token {
                            tok: Tok::Str(h), ..
                        }) => {
                            lower =
                                format!("{}{}", riddle_core::HEADER_PREFIX, h.to_ascii_lowercase());
                        }
                        _ => {
                            return Err(err(
                                line,
                                "header needs a name, like header X-GitHub-Reason",
                            ))
                        }
                    }
                }
                if self.eat_kw("CONTAINS") {
                    let value = match self.next() {
                        Some(Token {
                            tok: Tok::Str(s), ..
                        })
                        | Some(Token {
                            tok: Tok::Ident(s), ..
                        }) => s,
                        _ => {
                            return Err(err(
                                line,
                                format!("{name} CONTAINS needs text, like \"dependabot\""),
                            ))
                        }
                    };
                    return Ok(Expr::Fact {
                        fact: lower,
                        op: CmpOp::Contains,
                        value: FactValue::Text(value),
                    });
                }
                if self.eat_kw("IN") {
                    let list = match self.next() {
                        Some(Token { tok: Tok::Ident(l), .. }) if l.starts_with("list:") => {
                            return Ok(Expr::InNamedList {
                                subject: lower,
                                list: l["list:".len()..].to_string(),
                            });
                        }
                        Some(Token { tok: Tok::Ident(l), .. }) => NamedList::parse(&l).ok_or_else(|| err(line, format!("unknown list {l}; use never_hidden, contacts, vips, organisation, family, or one of yours as list:name")))?,
                        _ => return Err(err(line, "IN needs a list name")),
                    };
                    return Ok(Expr::InList {
                        subject: lower,
                        list,
                    });
                }
                if let Some(Token {
                    tok: Tok::Op(op), ..
                }) = self.peek().cloned()
                {
                    self.pos += 1;
                    let value = match self.next() {
                        Some(Token {
                            tok: Tok::Number(n),
                            ..
                        }) => FactValue::Number(n),
                        Some(Token {
                            tok: Tok::Str(s), ..
                        }) => FactValue::Text(s),
                        Some(Token {
                            tok: Tok::Ident(s), ..
                        }) => match s.to_ascii_lowercase().as_str() {
                            "true" => FactValue::Bool(true),
                            "false" => FactValue::Bool(false),
                            _ => FactValue::Text(s),
                        },
                        _ => {
                            return Err(err(line, format!("{name} {} needs a value", op.as_str())))
                        }
                    };
                    return Ok(Expr::Fact {
                        fact: lower,
                        op,
                        value,
                    });
                }
                Ok(Expr::Fact {
                    fact: lower,
                    op: CmpOp::Eq,
                    value: FactValue::Bool(true),
                })
            }
            Tok::Number(_) | Tok::Str(_) => {
                Err(err(line, "a condition starts with a fact or a signal"))
            }
            Tok::Op(op) => Err(err(line, format!("unexpected {}", op.as_str()))),
            Tok::RParen => Err(err(line, "unexpected ')'")),
            Tok::Arrow(_) => Err(err(line, "expected a condition before the arrow")),
        }
    }
}

fn parse_output(text: &str, line: usize) -> Result<MeshOutput> {
    let mut out = MeshOutput::default();
    if text.trim().is_empty() {
        return Err(err(line, "the arrow needs an output such as Find: Review, Filed, Spoil, Escalate or Stratum: Name"));
    }
    for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        let (head, tail) = match part.split_once(':') {
            Some((h, t)) => (h.trim().to_ascii_lowercase(), Some(t.trim())),
            None => (part.to_ascii_lowercase(), None),
        };
        let set_disposition = |out: &mut MeshOutput, d: Disposition| -> Result<()> {
            if out.disposition.is_some() {
                return Err(err(line, "an output has at most one disposition"));
            }
            out.disposition = Some(d);
            Ok(())
        };
        match (head.as_str(), tail) {
            ("find", Some(t)) => {
                let ft = FindType::parse(t).ok_or_else(|| err(line, format!("unknown Find type {t}; use Respond, Decide, Review, Do, Pay, Attend, Follow Up or Know")))?;
                set_disposition(&mut out, Disposition::Find { find_type: ft })?;
            }
            ("find", None) => return Err(err(line, "Find needs a type, e.g. Find: Review")),
            ("filed", None) => set_disposition(&mut out, Disposition::Filed)?,
            ("spoil", None) => set_disposition(&mut out, Disposition::Spoil)?,
            ("contamination", None) => set_disposition(&mut out, Disposition::Contamination)?,
            ("escalate", None) => out.escalate = true,
            ("notify", None) => out.notify = true,
            ("replaces", None) => out.replaces = true,
            ("replaces", Some(t)) => {
                if t.trim().eq_ignore_ascii_case("same subject") {
                    out.replaces = true;
                    out.replaces_scope = Some(riddle_core::SeriesScope::Subject);
                } else {
                    return Err(err(
                        line,
                        format!("Replaces is bare (the newest from the sender) or Replaces: same subject, not Replaces: {t}"),
                    ));
                }
            }
            ("expires", t) => out.expires = Some(parse_expiry(t.unwrap_or(""), line)?),
            ("importance", Some(t)) => {
                if out.importance.is_some() {
                    return Err(err(line, "an output has at most one Importance"));
                }
                out.importance = Some(Importance::parse(t).ok_or_else(|| {
                    err(
                        line,
                        format!("unknown Importance {t}; use Low, Normal, High or Critical"),
                    )
                })?);
            }
            ("importance", None) => {
                return Err(err(line, "Importance needs a level, e.g. Importance: High"))
            }
            ("stratum" | "strata", Some(t)) => {
                for s in t.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if !out.strata.iter().any(|x| x == s) {
                        out.strata.push(s.to_string());
                    }
                }
            }
            ("stratum" | "strata", None) => {
                return Err(err(line, "Stratum needs a name, e.g. Stratum: Receipts"))
            }
            _ => return Err(err(line, format!("unknown output {part:?}"))),
        }
    }
    if out.disposition.is_none() && !out.escalate && out.strata.is_empty() && !out.notify {
        return Err(err(line, "the output does nothing"));
    }
    // Replaces works where "only the newest matters" means something: a
    // Find (the older ones close) or Spoil and Filed (the older ones are
    // superseded: read, and below their list). Expires closes a Find.
    let is_find = matches!(out.disposition, Some(Disposition::Find { .. }));
    if out.replaces
        && !is_find
        && !matches!(
            out.disposition,
            Some(Disposition::Spoil | Disposition::Filed)
        )
    {
        return Err(err(
            line,
            "Replaces needs a Find, Spoil or Filed, e.g. Spoil + Replaces",
        ));
    }
    if out.expires.is_some() && !is_find {
        return Err(err(
            line,
            "Expires closes Finds, so it needs a Find, e.g. Find: Do + Expires",
        ));
    }
    Ok(out)
}

/// The most `Expires:` allows, in days; a Find that should outlive this
/// wants no expiry at all.
const MAX_EXPIRES_DAYS: u32 = 90;

/// `end of day`, `next morning`, or a span: `18h`, `18 hours`, `2d`,
/// `2 days`.
fn parse_expiry(text: &str, line: usize) -> Result<Expiry> {
    let t = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    match t.as_str() {
        "end of day" | "end of the day" => return Ok(Expiry::EndOfDay),
        "next morning" | "tomorrow morning" => return Ok(Expiry::NextMorning),
        "after its dates" | "after the dates" | "after booking" | "after the booking" => {
            return Ok(Expiry::AfterDates)
        }
        _ => {}
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    let (n, unit) = t.split_at(digits);
    if let "m" | "min" | "mins" | "minute" | "minutes" = unit.trim() {
        return match n.parse::<u32>() {
            Ok(minutes) if (1..60).contains(&minutes) => Ok(Expiry::AfterMinutes { minutes }),
            Ok(m) if m >= 60 && m % 60 == 0 => Ok(Expiry::After { hours: m / 60 }),
            _ => Err(err(
                line,
                "Expires in minutes is between 1 and 59, e.g. Expires: 15m",
            )),
        };
    }
    let per = match unit.trim() {
        "h" | "hr" | "hrs" | "hour" | "hours" => 1,
        "d" | "day" | "days" => 24,
        _ => {
            return Err(err(
                line,
                format!("Expires needs minutes, hours, days, end of day, next morning or after its dates, e.g. Expires: 18h or Expires: end of day, not {text:?}"),
            ))
        }
    };
    match n.parse::<u32>().ok().and_then(|n| n.checked_mul(per)) {
        Some(hours) if (1..=MAX_EXPIRES_DAYS * 24).contains(&hours) => Ok(Expiry::After { hours }),
        _ => Err(err(
            line,
            format!("Expires is between 1 hour and {MAX_EXPIRES_DAYS} days"),
        )),
    }
}

fn parse_scope(value: &str, line: usize) -> Result<MeshScope> {
    let v = value.trim();
    let lower = v.to_ascii_lowercase();
    if lower == "all digs" || lower == "all" || lower == "everywhere" {
        return Ok(MeshScope::AllDigs);
    }
    if let Some(g) = lower.strip_prefix("group:") {
        return Ok(MeshScope::DigGroup {
            group: v[v.len() - g.trim_start().len()..].trim().to_string(),
        });
    }
    if let Some(d) = lower.strip_prefix("dig:") {
        return Ok(MeshScope::Dig {
            dig_id: riddle_core::DigId(v[v.len() - d.trim_start().len()..].trim().to_string()),
        });
    }
    if let Some(g) = lower
        .strip_suffix(" digs")
        .or_else(|| lower.strip_suffix(" dig"))
    {
        return Ok(MeshScope::DigGroup {
            group: v[..g.len()].trim().to_string(),
        });
    }
    Err(err(
        line,
        format!("unknown scope {v:?}; use All Digs, Work Digs, Group: <name> or Dig: <id>"),
    ))
}

fn source_text(lines: &[(usize, &str)], from: usize, to: usize) -> String {
    lines
        .iter()
        .filter(|(n, _)| *n >= from && *n <= to)
        .map(|(_, l)| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse Mesh text into a Mesh. The id is fresh, the version is the header's
/// or 1, the status Draft; the service assigns the real id and version.
pub fn parse(text: &str) -> Result<Mesh> {
    parse_full(text).map(|(m, _)| m)
}

/// Parse and also return what the header said.
pub fn parse_full(text: &str) -> Result<(Mesh, ParsedHeader)> {
    let numbered: Vec<(usize, &str)> = text.lines().enumerate().map(|(i, l)| (i + 1, l)).collect();
    let section_of = |l: &str| -> Option<&'static str> {
        match l
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_uppercase()
            .as_str()
        {
            "ROUTES" | "ROUTES:" => Some("routes"),
            "GUARDS" | "GUARDS:" => Some("guards"),
            _ => None,
        }
    };
    let mut header: Vec<(usize, &str)> = Vec::new();
    let mut routes: Vec<(usize, &str)> = Vec::new();
    let mut guards: Vec<(usize, &str)> = Vec::new();
    let mut current = "header";
    for (n, l) in &numbered {
        if let Some(s) = section_of(l) {
            current = s;
            continue;
        }
        match current {
            "routes" => routes.push((*n, l)),
            "guards" => guards.push((*n, l)),
            _ => header.push((*n, l)),
        }
    }
    let parsed_header = parse_header(&header)?;

    // One IF … ELSE IF … ELSE chain per Mesh. A `WHEN <condition>` block
    // groups routes under a shared condition (sender_domain = github.com)
    // and closes with END or the next WHEN; its routes join the same chain
    // with the block condition ANDed in, and its ELSE is the block
    // condition alone, so a block can claim everything it matches.
    let mut route_list = Vec::new();
    let toks = tokenize(&routes)?;
    let mut p = Parser { toks, pos: 0 };
    let mut expect_else_chain = false;
    // (condition, source line) of the open block.
    let mut block: Option<(Expr, usize)> = None;
    let mut block_has_route = false;
    while let Some(t) = p.peek().cloned() {
        let start_line = t.line;
        if p.eat_kw("WHEN") {
            if block.is_some() && !block_has_route {
                return Err(err(
                    start_line,
                    "a WHEN block needs at least one route before the next WHEN",
                ));
            }
            let gate = p.expr()?;
            block = Some((gate, start_line));
            block_has_route = false;
            continue;
        }
        if p.eat_kw("END") {
            match block.take() {
                Some(_) if !block_has_route => {
                    return Err(err(start_line, "a WHEN block needs at least one route"))
                }
                Some(_) => continue,
                None => return Err(err(start_line, "END without a WHEN")),
            }
        }
        let first_in_block = block.is_some() && !block_has_route;
        let local = if p.eat_kw("IF") {
            if expect_else_chain && !first_in_block {
                return Err(err(
                    start_line,
                    "IF after a route; use ELSE IF to continue the chain, or start a WHEN block",
                ));
            }
            Some(p.expr()?)
        } else if p.eat_kw("ELSE") {
            if !expect_else_chain || first_in_block {
                return Err(err(start_line, "ELSE without a preceding IF"));
            }
            if p.eat_kw("IF") {
                Some(p.expr()?)
            } else {
                None
            }
        } else {
            return Err(err(start_line, "expected IF, ELSE, WHEN or END"));
        };
        expect_else_chain = true;
        let (output, end_line) = match p.next() {
            Some(Token {
                tok: Tok::Arrow(rest),
                line,
            }) => (parse_output(&rest, line)?, line),
            Some(Token { line, .. }) => return Err(err(line, "expected → after the condition")),
            None => return Err(err(p.line(), "expected → after the condition")),
        };
        let own_text = source_text(&routes, start_line, end_line);
        let (condition, text, when) = match &block {
            Some((gate, gate_line)) => {
                let condition = match &local {
                    Some(l) => Expr::And {
                        items: vec![gate.clone(), l.clone()],
                    },
                    None => gate.clone(),
                };
                let gate_text = source_text(&routes, *gate_line, *gate_line);
                (
                    condition,
                    format!("{} ▸ {}", gate_text, own_text.trim()),
                    Some(gate.clone()),
                )
            }
            None => (local.clone().unwrap_or(Expr::Always), own_text, None),
        };
        block_has_route = block.is_some();
        let mut route = Route {
            condition,
            output,
            text,
            local: if when.is_some() { local } else { None },
            when,
            words: String::new(),
        };
        route.words = crate::humanize::route_reading(&route);
        route_list.push(route);
    }
    if block.is_some() && !block_has_route {
        return Err(err(p.line(), "a WHEN block needs at least one route"));
    }

    let mut guard_list = Vec::new();
    let toks = tokenize(&guards)?;
    let mut p = Parser { toks, pos: 0 };
    while let Some(t) = p.peek().cloned() {
        let start_line = t.line;
        if !p.eat_kw("NEVER") {
            return Err(err(
                start_line,
                "expected NEVER <Spoil|Contamination|Filed|Find> IF <condition>",
            ));
        }
        let never = match p.next() {
            Some(Token {
                tok: Tok::Ident(k),
                line,
            }) => match k.to_ascii_lowercase().as_str() {
                "spoil" => DispositionKind::Spoil,
                "contamination" => DispositionKind::Contamination,
                "filed" => DispositionKind::Filed,
                "find" => DispositionKind::Find,
                other => {
                    return Err(err(
                        line,
                        format!("NEVER {other} is not a disposition a guard can veto"),
                    ))
                }
            },
            _ => return Err(err(start_line, "NEVER needs a disposition")),
        };
        if !p.eat_kw("IF") {
            return Err(err(start_line, "NEVER <disposition> needs IF <condition>"));
        }
        let condition = p.expr()?;
        let end_line = p
            .toks
            .get(p.pos.saturating_sub(1))
            .map(|t| t.line)
            .unwrap_or(start_line);
        let mut guard = Guard {
            never,
            condition,
            text: source_text(&guards, start_line, end_line),
            words: String::new(),
        };
        guard.words = crate::humanize::guard_reading(&guard);
        guard_list.push(guard);
    }

    if route_list.is_empty() && guard_list.is_empty() {
        return Err(err(1, "a Mesh needs at least one route or guard"));
    }

    let mesh = Mesh {
        id: MeshId::new(),
        name: parsed_header
            .name
            .clone()
            .unwrap_or_else(|| "Untitled Mesh".into()),
        version: parsed_header.version.unwrap_or(1),
        scope: parsed_header.scope.clone(),
        priority: parsed_header.priority,
        routes: route_list,
        guards: guard_list,
        status: MeshStatus::Draft,
        source_text: text.to_string(),
        created_at: chrono::Utc::now(),
        created_by: CreatedBy::User,
        change_note: None,
        shadow_started_at: None,
        shadow_agreed: 0,
        shadow_proposed: 0,
        deleted_at: None,
        autonomy: parsed_header.autonomy,
        demoted: None,
        fallback: false,
        pack: None,
    };
    Ok((mesh, parsed_header))
}

fn parse_header(lines: &[(usize, &str)]) -> Result<ParsedHeader> {
    let mut h = ParsedHeader {
        name: None,
        version: None,
        scope: MeshScope::AllDigs,
        scope_given: false,
        priority: 0,
        priority_given: false,
        autonomy: None,
        autonomy_given: false,
    };
    let mut expecting_scope = false;
    for (n, raw) in lines {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if expecting_scope {
            h.scope = parse_scope(line, *n)?;
            h.scope_given = true;
            expecting_scope = false;
            continue;
        }
        if let Some(rest) = lower.strip_prefix("mesh:") {
            let rest = line[line.len() - rest.len()..].trim();
            let (name, version) = match rest.rsplit_once(" v") {
                Some((nm, v)) if v.chars().all(|c| c.is_ascii_digit()) && !v.is_empty() => {
                    (nm.trim().to_string(), v.parse().ok())
                }
                _ => (rest.to_string(), None),
            };
            if name.is_empty() {
                return Err(err(*n, "Mesh: needs a name"));
            }
            h.name = Some(name);
            h.version = version;
        } else if let Some(rest) = lower.strip_prefix("scope:") {
            if rest.trim().is_empty() {
                expecting_scope = true;
            } else {
                h.scope = parse_scope(line[line.len() - rest.len()..].trim(), *n)?;
                h.scope_given = true;
            }
        } else if let Some(rest) = lower.strip_prefix("priority:") {
            h.priority = rest
                .trim()
                .parse()
                .map_err(|_| err(*n, "Priority: needs an integer"))?;
            h.priority_given = true;
        } else if let Some(rest) = lower.strip_prefix("autonomy:") {
            h.autonomy = Some(match rest.trim() {
                "observe" => Autonomy::Observe,
                "suggest" => Autonomy::Suggest,
                "act" => Autonomy::Act,
                "trusted" => {
                    return Err(err(
                        *n,
                        "Autonomy: a Mesh can be capped at Observe, Suggest or Act; Trusted is an account setting",
                    ))
                }
                other => {
                    return Err(err(
                        *n,
                        format!("Autonomy: {other:?} is not one of Observe, Suggest or Act"),
                    ))
                }
            });
            h.autonomy_given = true;
        } else {
            return Err(err(*n, format!("unexpected header line {line:?}; expected Mesh:, Scope:, Priority:, Autonomy:, ROUTES or GUARDS")));
        }
    }
    if expecting_scope {
        return Err(err(
            lines.last().map(|l| l.0).unwrap_or(1),
            "Scope: needs a value on the next line",
        ));
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_when_block_formats_back_as_a_block() {
        let text = "Mesh: GitHub v2\n\nROUTES\n\nWHEN sender_domain = github.com\n    IF sender_name CONTAINS \"dependabot\" → Spoil\n    ELSE → Filed\nEND\n\nELSE IF sender_domain = example.com → Find: Respond\n";
        let m = parse(text).unwrap();
        assert_eq!(m.routes.len(), 3);
        assert!(m.routes[0].when.is_some() && m.routes[1].local.is_none());
        assert!(m.routes[2].when.is_none());
        let formatted = crate::format::format(&m);
        assert!(
            formatted.contains("WHEN sender_domain = github.com"),
            "{formatted}"
        );
        assert!(formatted.contains("END"), "{formatted}");
        let again = parse(&formatted).unwrap();
        let logic = |m: &Mesh| {
            m.routes
                .iter()
                .map(|r| {
                    (
                        r.condition.clone(),
                        r.output.clone(),
                        r.when.clone(),
                        r.local.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(logic(&again), logic(&m));
    }

    #[test]
    fn when_blocks_reject_malformed_structure() {
        assert!(
            parse("ROUTES\nWHEN sender_domain = github.com\nEND").is_err(),
            "empty block"
        );
        assert!(parse("ROUTES\nEND").is_err(), "END without WHEN");
        assert!(
            parse("ROUTES\nWHEN sender_domain = github.com\n ELSE → Spoil").is_err(),
            "ELSE first in a block"
        );
    }

    #[test]
    fn header_facts_and_contains_parse_and_format_back() {
        let text = "Mesh: GitHub v1\n\nROUTES\n\nIF sender_name CONTAINS \"dependabot\" → Spoil\n\nELSE IF header X-GitHub-Reason = ci_activity → Filed\n";
        let m = parse(text).unwrap();
        assert_eq!(
            m.routes[0].condition,
            Expr::Fact {
                fact: "sender_name".into(),
                op: CmpOp::Contains,
                value: FactValue::Text("dependabot".into())
            }
        );
        assert_eq!(
            m.routes[1].condition,
            Expr::Fact {
                fact: "header:x-github-reason".into(),
                op: CmpOp::Eq,
                value: FactValue::Text("ci_activity".into())
            }
        );
        let again = parse(&crate::format::format(&m)).unwrap();
        let logic = |m: &Mesh| {
            m.routes
                .iter()
                .map(|r| (r.condition.clone(), r.output.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(logic(&again), logic(&m));
    }

    #[test]
    fn replaces_parses_formats_back_and_reads_as_a_sentence() {
        let text = "Mesh: Daily reminders v1\n\nROUTES\n\nIF subject CONTAINS \"Time to write\" → Find: Do + Replaces\n";
        let m = parse(text).unwrap();
        assert!(m.routes[0].output.replaces);
        let formatted = crate::format::format(&m);
        assert!(formatted.contains("Find: Do + Replaces"), "{formatted}");
        assert_eq!(
            parse(&formatted).unwrap().routes[0].output,
            m.routes[0].output
        );
        assert!(
            m.routes[0]
                .words
                .contains("a newer one from the same sender replaces it"),
            "{}",
            m.routes[0].words
        );
    }

    #[test]
    fn replaces_same_subject_parses_formats_back_and_reads_as_a_sentence() {
        let text = "Mesh: Renewals v1\n\nROUTES\n\nIF subject CONTAINS \"expires in\" → Find: Do + Replaces: same subject\n";
        let m = parse(text).unwrap();
        let o = &m.routes[0].output;
        assert!(o.replaces);
        assert_eq!(o.series_scope(), riddle_core::SeriesScope::Subject);
        let formatted = crate::format::format(&m);
        assert!(
            formatted.contains("Find: Do + Replaces: same subject"),
            "{formatted}"
        );
        assert_eq!(parse(&formatted).unwrap().routes[0].output, *o);
        assert!(
            m.routes[0]
                .words
                .contains("a newer one from the same sender on the same subject replaces it"),
            "{}",
            m.routes[0].words
        );
        let e =
            parse("Mesh: X v1\n\nROUTES\n\nIF mailing_list → Find: Do + Replaces: same thread\n")
                .unwrap_err();
        assert!(
            e.message.contains("Replaces: same subject"),
            "{}",
            e.message
        );
    }

    #[test]
    fn replaces_where_newest_means_nothing_is_rejected() {
        let e =
            parse("ROUTES\nIF subject CONTAINS \"digest\" → Contamination + Replaces").unwrap_err();
        assert!(e.message.contains("needs a Find"), "{}", e.message);
    }

    #[test]
    fn importance_and_a_minute_expiry_parse_read_as_words_and_format_back() {
        let text = "Mesh: Codes\nROUTES\nIF subject CONTAINS \"code\" → Find: Know + Notify + Expires: 15 minutes + Importance: critical\n";
        let m = parse(text).unwrap();
        let o = &m.routes[0].output;
        assert_eq!(o.importance, Some(Importance::Critical));
        assert_eq!(o.expires, Some(Expiry::AfterMinutes { minutes: 15 }));
        let formatted = crate::format::format(&m);
        assert!(
            formatted.contains("Find: Know + Notify + Expires: 15m + Importance: Critical"),
            "{formatted}"
        );
        assert_eq!(parse(&formatted).unwrap().routes[0].output, *o);
        let words = &m.routes[0].words;
        assert!(words.contains("of critical importance"), "{words}");
        assert!(
            words.contains("expires 15 minutes after it arrives"),
            "{words}"
        );
        // 120m is 2h, and still only for a Find.
        let e = parse("ROUTES\nIF subject CONTAINS \"x\" → Filed + Expires: 120m").unwrap_err();
        assert!(e.message.contains("needs a Find"), "{}", e.message);
        let e =
            parse("ROUTES\nIF subject CONTAINS \"x\" → Filed + Importance: urgent").unwrap_err();
        assert!(
            e.message.contains("Low, Normal, High or Critical"),
            "{}",
            e.message
        );
    }

    #[test]
    fn expires_takes_hours_or_days_and_formats_back_in_the_shortest_form() {
        let text = "Mesh: Daily reminders v1\nROUTES\nIF subject CONTAINS \"Time to write\" → Find: Do + Replaces + Expires: 2 days\nELSE IF subject CONTAINS \"Prompt\" → Find: Do + Expires: 18h\n";
        let m = parse(text).unwrap();
        assert_eq!(
            m.routes[0].output.expires,
            Some(Expiry::After { hours: 48 })
        );
        assert_eq!(
            m.routes[1].output.expires,
            Some(Expiry::After { hours: 18 })
        );
        let formatted = crate::format::format(&m);
        assert!(
            formatted.contains("Find: Do + Replaces + Expires: 2d"),
            "{formatted}"
        );
        assert!(formatted.contains("Find: Do + Expires: 18h"), "{formatted}");
        assert_eq!(
            parse(&formatted).unwrap().routes[0].output,
            m.routes[0].output
        );
        assert!(
            m.routes[0]
                .words
                .contains("it expires 2 days after it arrives"),
            "{}",
            m.routes[0].words
        );
        assert!(
            m.routes[1]
                .words
                .contains("it expires 18 hours after it arrives"),
            "{}",
            m.routes[1].words
        );
    }

    #[test]
    fn expires_at_the_end_of_the_day_or_the_next_morning_reads_as_a_sentence() {
        let m = parse("ROUTES\nIF mailing_list → Find: Do + Expires: End of the day\nELSE → Find: Do + Expires: tomorrow morning").unwrap();
        assert_eq!(m.routes[0].output.expires, Some(Expiry::EndOfDay));
        assert!(
            m.routes[0]
                .words
                .contains("it expires at the end of the day it arrives"),
            "{}",
            m.routes[0].words
        );
        assert!(
            m.routes[1]
                .words
                .contains("it expires at 9 am the next morning"),
            "{}",
            m.routes[1].words
        );
        let formatted = crate::format::format(&m);
        assert!(
            formatted.contains("Expires: end of day")
                && formatted.contains("Expires: next morning"),
            "{formatted}"
        );
    }

    #[test]
    fn a_cadence_rule_reads_as_how_often_the_sender_writes() {
        let m = parse("ROUTES\nIF sender_cadence = daily AND sender_on_schedule AND personally_relevant@v1 > .8 → Find: Do + Replaces + Expires: end of day\nELSE IF sender_cadence != weekdays → Filed").unwrap();
        assert!(
            m.routes[0].words.contains("the sender writes daily"),
            "{}",
            m.routes[0].words
        );
        assert!(
            m.routes[0]
                .words
                .contains("the sender writes at the same time each day"),
            "{}",
            m.routes[0].words
        );
        assert!(
            m.routes[1]
                .words
                .contains("the sender does not write on weekdays"),
            "{}",
            m.routes[1].words
        );
    }

    #[test]
    fn expires_rejects_what_is_not_a_bounded_duration_or_not_a_find() {
        for bad in [
            "Expires: later",
            "Expires: 0h",
            "Expires: 91d",
            "Expires: 1.5d",
            "Expires",
        ] {
            let text = format!("ROUTES\nIF mailing_list → Find: Do + {bad}");
            assert!(parse(&text).is_err(), "{bad} should not parse");
        }
        let e = parse("ROUTES\nIF mailing_list → Filed + Expires: 2d").unwrap_err();
        assert!(e.message.contains("Expires closes Finds"), "{}", e.message);
    }

    #[test]
    fn header_needs_a_name() {
        assert!(parse("ROUTES\nIF header = x → Spoil").is_err());
    }

    use super::*;

    pub const DOC_EXAMPLE: &str = "Mesh: GitHub Notifications v8\n\nScope:\nWork Digs\n\nROUTES\n\nIF review_requested@v3 > .80\n    → Find: Review\n\nELSE IF ci_failure@v4 > .85\n     AND personally_relevant@v3 > .70\n    → Find: Do\n\nELSE IF routine@v1 > .90\n     AND personally_relevant@v3 < .30\n    → Spoil\n\nELSE\n    → Escalate\n\nGUARDS\n\nNEVER Spoil IF sender IN never_hidden\n";

    #[test]
    fn the_doc_example_parses_into_the_expected_ast() {
        let m = parse(DOC_EXAMPLE).unwrap();
        assert_eq!(m.name, "GitHub Notifications");
        assert_eq!(m.version, 8);
        assert_eq!(
            m.scope,
            MeshScope::DigGroup {
                group: "Work".into()
            }
        );
        assert_eq!(m.routes.len(), 4);
        assert_eq!(
            m.routes[0].condition,
            Expr::Signal {
                signal: "review_requested".into(),
                version: 3,
                choice: None,
                op: CmpOp::Gt,
                threshold: 0.80
            }
        );
        assert_eq!(
            m.routes[0].output.disposition,
            Some(Disposition::Find {
                find_type: FindType::Review
            })
        );
        assert!(matches!(&m.routes[1].condition, Expr::And { items } if items.len() == 2));
        assert_eq!(m.routes[3].condition, Expr::Always);
        assert!(m.routes[3].output.escalate);
        assert_eq!(m.guards.len(), 1);
        assert_eq!(m.guards[0].never, DispositionKind::Spoil);
        assert_eq!(
            m.guards[0].condition,
            Expr::InList {
                subject: "sender".into(),
                list: NamedList::NeverHidden
            }
        );
        assert_eq!(
            m.routes[1].text,
            "ELSE IF ci_failure@v4 > .85 AND personally_relevant@v3 > .70 → Find: Do"
        );
    }

    #[test]
    fn keywords_are_case_insensitive_and_both_arrows_work() {
        let m = parse("mesh: Receipts\nscope: all digs\nroutes\nif has_structured_receipt -> Filed + Stratum: Purchases + Stratum: Receipts\nelse if receipt@v1 >= 0.85 and not mailing_list -> filed + stratum: Receipts\nguards\nnever spoil if sender in contacts").unwrap();
        assert_eq!(
            m.routes[0].output.strata,
            vec!["Purchases".to_string(), "Receipts".to_string()]
        );
        assert_eq!(
            m.routes[0].condition,
            Expr::Fact {
                fact: "has_structured_receipt".into(),
                op: CmpOp::Eq,
                value: FactValue::Bool(true)
            }
        );
        assert!(
            matches!(&m.routes[1].condition, Expr::And { items } if matches!(items[1], Expr::Not { .. }))
        );
        assert_eq!(
            m.guards[0].condition,
            Expr::InList {
                subject: "sender".into(),
                list: NamedList::Contacts
            }
        );
    }

    #[test]
    fn errors_carry_the_line_number() {
        let e = parse("Mesh: X\nROUTES\nIF review_requested@v1 > .8\n    → Find: Nonsense")
            .unwrap_err();
        assert_eq!(e.kind, riddle_errors::ErrorKind::Invalid);
        assert_eq!(e.detail.as_ref().unwrap()["line"], 4);
        let e = parse("Mesh: X\nROUTES\nELSE → Spoil").unwrap_err();
        assert_eq!(e.detail.as_ref().unwrap()["line"], 3);
        let e = parse("Mesh: X\nROUTES\nIF gmail_category = promotions AND thread_length > 3 → Spoil\nGUARDS\nNEVER Spoil IF sender IN nobody").unwrap_err();
        assert_eq!(e.detail.as_ref().unwrap()["line"], 5);
    }

    #[test]
    fn fact_comparisons_and_choices_parse() {
        let m = parse("ROUTES\nIF gmail_category = promotions AND thread_length > 3 AND kind@v2:INVOICE >= 0.5 AND sender_domain = \"example.org\" → Spoil").unwrap();
        let Expr::And { items } = &m.routes[0].condition else {
            panic!()
        };
        assert_eq!(
            items[0],
            Expr::Fact {
                fact: "gmail_category".into(),
                op: CmpOp::Eq,
                value: FactValue::Text("promotions".into())
            }
        );
        assert_eq!(
            items[1],
            Expr::Fact {
                fact: "thread_length".into(),
                op: CmpOp::Gt,
                value: FactValue::Number(3.0)
            }
        );
        assert_eq!(
            items[2],
            Expr::Signal {
                signal: "kind".into(),
                version: 2,
                choice: Some("INVOICE".into()),
                op: CmpOp::Gte,
                threshold: 0.5
            }
        );
        assert_eq!(
            items[3],
            Expr::Fact {
                fact: "sender_domain".into(),
                op: CmpOp::Eq,
                value: FactValue::Text("example.org".into())
            }
        );
        assert_eq!(m.name, "Untitled Mesh");
    }
}
