//! "Continue for 20", "keep going": the person lifting a turn's limits in
//! words. The round limit exists to stop a runaway turn; a turn that stopped
//! doing useful work ended with "ask me to continue", and the person had no
//! way to say "and don't stop again so soon". Read here without a model, so
//! the words mean exactly what this file says they mean; a message that says
//! anything more ("continue with the GitHub Mesh") is an ordinary message.

use riddle_core::ToolAllowance;

/// Fold what a continue message asked for into the conversation's
/// allowance: "keep going" lasts the conversation, the rest is for the next
/// turn only.
pub fn fold(prev: Option<ToolAllowance>, asked: ToolAllowance) -> ToolAllowance {
    ToolAllowance {
        keep_going: asked.keep_going || prev.is_some_and(|p| p.keep_going),
        ..asked
    }
}

/// `continue`, `go on`, `carry on`, `keep going`, optionally followed by
/// `for N [more] [tool] [calls|rounds|steps]`, `until [a] new conversation
/// [starts]`, and `[up to|with] $X [more]`, in any order.
pub fn parse(message: &str) -> Option<ToolAllowance> {
    let lower = message
        .trim()
        .trim_end_matches(['.', '!'])
        .to_lowercase()
        .replace(',', " ");
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut rest = words.as_slice();
    for lead in ["please", "ok", "okay", "yes"] {
        if rest.first() == Some(&lead) {
            rest = &rest[1..];
        }
    }
    let mut req = ToolAllowance::default();
    rest = match rest {
        ["continue", r @ ..] => r,
        ["go" | "carry", "on", r @ ..] => r,
        ["keep", "going", r @ ..] => {
            req.keep_going = true;
            r
        }
        _ => return None,
    };
    while !rest.is_empty() {
        rest = match rest {
            ["for", n, r @ ..] => {
                req.next_turn_tool_calls = Some(n.parse().ok().filter(|n: &u32| *n > 0)?);
                let r = r.strip_prefix(&["more"]).unwrap_or(r);
                let r = r.strip_prefix(&["tool"]).unwrap_or(r);
                match r {
                    ["calls" | "call" | "rounds" | "steps", r @ ..] => r,
                    r => r,
                }
            }
            ["until", r @ ..] => {
                let r = r.strip_prefix(&["a"]).unwrap_or(r);
                match r {
                    ["new", "conversation", r @ ..] => {
                        req.keep_going = true;
                        r.strip_prefix(&["starts"]).unwrap_or(r)
                    }
                    _ => return None,
                }
            }
            ["up", "to", r @ ..] | ["with", "up", "to", r @ ..] | ["with", r @ ..] => {
                let (cost, r) = dollars(r)?;
                req.next_turn_cost_usd = Some(cost);
                r
            }
            r if r[0].starts_with('$') => {
                let (cost, r) = dollars(r)?;
                req.next_turn_cost_usd = Some(cost);
                r
            }
            _ => return None,
        };
    }
    Some(req)
}

/// `$2`, `$1.50`, optionally followed by `more`.
fn dollars<'a>(words: &'a [&'a str]) -> Option<(f64, &'a [&'a str])> {
    let (first, r) = words.split_first()?;
    let cost: f64 = first.strip_prefix('$')?.parse().ok()?;
    if !(cost > 0.0 && cost.is_finite()) {
        return None;
    }
    Some((cost, r.strip_prefix(&["more"]).unwrap_or(r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(
        keep_going: bool,
        tool_calls: Option<u32>,
        cost_usd: Option<f64>,
    ) -> Option<ToolAllowance> {
        Some(ToolAllowance {
            keep_going,
            next_turn_tool_calls: tool_calls,
            next_turn_cost_usd: cost_usd,
        })
    }

    #[test]
    fn the_ways_to_say_continue_read_as_the_room_they_ask_for() {
        assert_eq!(parse("continue"), req(false, None, None));
        assert_eq!(parse("  Please continue. "), req(false, None, None));
        assert_eq!(parse("go on!"), req(false, None, None));
        assert_eq!(parse("continue for 20"), req(false, Some(20), None));
        assert_eq!(
            parse("continue for 20 more tool calls"),
            req(false, Some(20), None)
        );
        assert_eq!(parse("keep going"), req(true, None, None));
        assert_eq!(
            parse("continue until a new conversation starts"),
            req(true, None, None)
        );
        assert_eq!(
            parse("continue for 30, up to $2"),
            req(false, Some(30), Some(2.0))
        );
        assert_eq!(
            parse("keep going with $1.50 more"),
            req(true, None, Some(1.5))
        );
    }

    #[test]
    fn a_message_that_says_more_than_continue_is_an_ordinary_message() {
        for m in [
            "continue with the GitHub Mesh",
            "continue for a while",
            "continue for 0",
            "keep going until tomorrow",
            "continue up to $0",
            "why did you stop?",
            "",
        ] {
            assert_eq!(parse(m), None, "{m:?}");
        }
    }

    #[test]
    fn keep_going_lasts_the_conversation_and_the_rest_only_the_next_turn() {
        let a = fold(None, parse("keep going").unwrap());
        assert!(a.keep_going);
        let a = fold(Some(a), parse("continue for 10").unwrap());
        assert!(a.keep_going, "still keeping going");
        assert_eq!(a.next_turn_tool_calls, Some(10));
        let a = fold(Some(a), parse("continue").unwrap());
        assert_eq!(
            a.next_turn_tool_calls, None,
            "a plain continue asks for no more"
        );
    }
}
