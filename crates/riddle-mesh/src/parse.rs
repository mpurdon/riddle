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
    CmpOp, CreatedBy, Disposition, DispositionKind, Expr, FactValue, FindType, Guard, Mesh, MeshId,
    MeshOutput, MeshScope, MeshStatus, NamedList, Route,
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
                let lower = name.to_ascii_lowercase();
                if lower == "true" || lower == "false" {
                    return Err(err(line, "a bare true/false is meaningless here"));
                }
                if self.eat_kw("IN") {
                    let list = match self.next() {
                        Some(Token { tok: Tok::Ident(l), .. }) => NamedList::parse(&l).ok_or_else(|| err(line, format!("unknown list {l}; use never_hidden, contacts, vips, organisation or family")))?,
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
    Ok(out)
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

    let mut route_list = Vec::new();
    let toks = tokenize(&routes)?;
    let mut p = Parser { toks, pos: 0 };
    let mut expect_else_chain = false;
    while let Some(t) = p.peek().cloned() {
        let start_line = t.line;
        let condition = if p.eat_kw("IF") {
            if expect_else_chain {
                return Err(err(
                    start_line,
                    "IF after a route; use ELSE IF to continue the chain or start a new Mesh",
                ));
            }
            expect_else_chain = true;
            p.expr()?
        } else if p.eat_kw("ELSE") {
            if !expect_else_chain {
                return Err(err(start_line, "ELSE without a preceding IF"));
            }
            if p.eat_kw("IF") {
                p.expr()?
            } else {
                Expr::Always
            }
        } else {
            return Err(err(start_line, "expected IF or ELSE"));
        };
        let (output, end_line) = match p.next() {
            Some(Token {
                tok: Tok::Arrow(rest),
                line,
            }) => (parse_output(&rest, line)?, line),
            Some(Token { line, .. }) => return Err(err(line, "expected → after the condition")),
            None => return Err(err(p.line(), "expected → after the condition")),
        };
        let text = source_text(&routes, start_line, end_line);
        route_list.push(Route {
            condition,
            output,
            text,
        });
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
        guard_list.push(Guard {
            never,
            condition,
            text: source_text(&guards, start_line, end_line),
        });
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
        } else {
            return Err(err(*n, format!("unexpected header line {line:?}; expected Mesh:, Scope:, Priority:, ROUTES or GUARDS")));
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
