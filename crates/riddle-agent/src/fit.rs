//! Fitting a tool result into the model's budget without lying about it.
//!
//! A result over `MAX_RESULT_CHARS` used to be cut mid-JSON with a note to
//! narrow the query. The cut fell after the rows and before `total` and
//! `nextCursor`, so the Director could not tell how much it had missed: on
//! a real profile it saw 15 of 50 Activity rows, 22 of 181 open Finds, and
//! none of the Payments Mesh it was editing (12 Oct 2026). Now a result is
//! made smaller in ways that keep it valid and say what went:
//!
//! 1. a string longer than a cap (a message body) keeps its start and says
//!    how much more there was;
//! 2. a list keeps whole rows from the front and gains `shown`, `omitted`
//!    and a `note`, beside the `total` and `nextCursor` the page already had;
//! 3. strings are capped harder until it fits;
//! 4. failing all that, the start of the result as text, marked partial.

use serde_json::{json, Map, Value};

/// The longest a string stays whole before anything else is tried.
const FIRST_STRING_CAP: usize = 8_000;

/// The caps tried, in turn, when whole rows are not enough.
const STRING_CAPS: [usize; 4] = [4_000, 2_000, 1_000, 400];

fn size(v: &Value) -> usize {
    // Bytes, which are never fewer than characters: under the budget in
    // bytes is under it in characters.
    v.to_string().len()
}

/// `v`, made to fit `max` characters as JSON. Each step works on what the
/// last one left, and the first result that fits is returned.
pub fn fit(v: Value, max: usize) -> Value {
    if size(&v) <= max {
        return v;
    }
    let mut v = cap_strings(v, FIRST_STRING_CAP);
    if size(&v) <= max {
        return v;
    }
    v = drop_rows(v, max);
    for cap in STRING_CAPS {
        if size(&v) <= max {
            return v;
        }
        v = cap_strings(v, cap);
    }
    if size(&v) <= max {
        return v;
    }
    as_partial_text(&v, max)
}

/// The last resort: the start of the result as quoted text. Escaping makes
/// it longer, so it is shortened until it fits.
fn as_partial_text(v: &Value, max: usize) -> Value {
    let text = v.to_string();
    let mut cut = max.saturating_sub(300).min(text.len());
    loop {
        cut = riddle_core::floor_char_boundary(&text, cut);
        let out = json!({
            "partial": &text[..cut],
            "note": format!("Only the first {cut} of {} characters fit; ask for less (a page, one item, a narrower query).", text.len()),
        });
        if size(&out) <= max || cut == 0 {
            return out;
        }
        cut = cut * 9 / 10;
    }
}

/// Every string longer than `cap` keeps its first `cap` characters and says
/// how many more there were.
fn cap_strings(v: Value, cap: usize) -> Value {
    match v {
        Value::String(s) => {
            let n = s.chars().count();
            if n <= cap {
                Value::String(s)
            } else {
                let head: String = s.chars().take(cap).collect();
                Value::String(format!("{head}… [continues: {} more characters]", n - cap))
            }
        }
        Value::Array(a) => Value::Array(a.into_iter().map(|x| cap_strings(x, cap)).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, x)| (k, cap_strings(x, cap)))
                .collect(),
        ),
        other => other,
    }
}

/// Keeps whole rows from the front of the result's list (a bare array, or
/// the longest array field of an object, such as a page's `items`) until it
/// fits, and records what was left out. A bare list becomes a page, so it
/// has somewhere to say so.
fn drop_rows(v: Value, max: usize) -> Value {
    let (mut o, field, rows) = match v {
        Value::Array(rows) => (Map::new(), "items".to_string(), rows),
        Value::Object(mut o) => {
            let longest = o
                .iter()
                .filter_map(|(k, x)| x.as_array().map(|a| (k.clone(), a.len())))
                .filter(|(_, n)| *n > 1)
                .max_by_key(|(_, n)| *n)
                .map(|(k, _)| k);
            let Some(field) = longest else {
                return Value::Object(o);
            };
            let rows = match o.remove(&field) {
                Some(Value::Array(rows)) => rows,
                _ => vec![],
            };
            (o, field, rows)
        }
        other => return other,
    };
    let all = rows.len();
    // The envelope without rows, with room for the counts and the note.
    let mut used = size(&Value::Object(o.clone())) + field.len() + 400;
    let keep = rows
        .iter()
        .take_while(|r| {
            used += size(r) + 1;
            used <= max
        })
        .count();
    let mut kept = rows;
    kept.truncate(keep);
    o.insert(field, Value::Array(kept));
    if keep < all {
        let paged = o.get("nextCursor").is_some_and(|c| !c.is_null()) || o.get("total").is_some();
        let more = if paged {
            "Counts come from `total`; ask for the next page or a narrower query to see the rest."
        } else {
            "Ask for a narrower query to see the rest."
        };
        o.insert("shown".into(), json!(keep));
        o.insert("omitted".into(), json!(all - keep));
        o.insert(
            "note".into(),
            json!(format!(
                "Only the first {keep} of {all} rows fit in this result. {more}"
            )),
        );
    }
    Value::Object(o)
}

/// The rows of a page (`items`) or a bare list.
pub fn rows(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::Array(a) => Some(a),
        Value::Object(o) => o.get("items").and_then(|i| i.as_array()),
        _ => None,
    }
}

/// Only these fields of an object.
pub fn pick(v: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|k| v.get(*k).map(|x| ((*k).to_string(), x.clone())))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(i: usize) -> Value {
        json!({ "id": format!("m{i}"), "subject": "x".repeat(900) })
    }

    #[test]
    fn a_result_that_fits_is_left_alone() {
        let v = json!({ "items": [row(1)], "total": 1 });
        assert_eq!(fit(v.clone(), 30_000), v);
    }

    #[test]
    fn a_page_keeps_whole_rows_and_says_how_many_it_left_out() {
        let v = json!({ "items": (0..50).map(row).collect::<Vec<_>>(), "total": 181, "nextCursor": "c50" });
        let out = fit(v, 30_000);
        assert!(out.to_string().len() <= 30_000);
        let shown = out["shown"].as_u64().unwrap() as usize;
        assert_eq!(out["items"].as_array().unwrap().len(), shown);
        assert_eq!(shown + out["omitted"].as_u64().unwrap() as usize, 50);
        assert_eq!(
            (out["total"].clone(), out["nextCursor"].clone()),
            (json!(181), json!("c50"))
        );
        assert_eq!(
            out["items"][0]["id"], "m0",
            "rows are kept from the front, whole"
        );
        assert!(out["note"].as_str().unwrap().contains("total"));
    }

    #[test]
    fn a_bare_list_becomes_a_page_with_its_counts() {
        let v = Value::Array((0..60).map(row).collect());
        let out = fit(v, 30_000);
        assert_eq!(
            out["omitted"].as_u64().unwrap() + out["shown"].as_u64().unwrap(),
            60
        );
    }

    #[test]
    fn a_long_body_keeps_its_start_and_says_how_much_more_there_was() {
        let v = json!({ "id": "m1", "bodyText": "y".repeat(40_000) });
        let out = fit(v, 30_000);
        let body = out["bodyText"].as_str().unwrap();
        assert!(
            body.starts_with("yyyy") && body.ends_with("[continues: 32000 more characters]"),
            "{}",
            &body[body.len() - 60..]
        );
        assert_eq!(out["id"], "m1");
    }

    #[test]
    fn whatever_the_shape_the_result_is_valid_json_under_the_budget() {
        let nested = json!({ "a": { "b": (0..400).map(|i| json!({ "k": i, "s": "z".repeat(300) })).collect::<Vec<_>>() }, "c": "q".repeat(50_000) });
        for v in [nested, Value::String("w".repeat(90_000))] {
            let out = fit(v, 30_000);
            assert!(out.to_string().len() <= 30_000, "{}", out.to_string().len());
        }
    }
}
