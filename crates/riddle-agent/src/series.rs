//! Series discovery from open Finds, the counterpart of `corrections` for
//! piles rather than corrections: a sender whose open Finds repeat one
//! subject, numbers aside, under a route without `+ Replaces` accumulates
//! into a suggestion the Director can voice ("33 open Finds from
//! noreply@august.com say the same thing; I can propose `+ Replaces: same
//! subject` on the route"). The pattern had to be noticed by hand twice on
//! one day (16 Oct 2026). Pure: it reads rows the app assembles and
//! produces text; nothing changes a Mesh without the user's yes.

use riddle_core::{series_subject, FindId, MeshId, SeriesSuggestion};
use std::collections::BTreeMap;

/// Open Finds on one subject from one sender before a suggestion is made.
pub const SUGGEST_AT: usize = 3;

/// One open Find as the detector sees it: the app assembles these from the
/// Find, its source message and the decision that raised it.
#[derive(Debug, Clone)]
pub struct OpenFindRow {
    pub find_id: FindId,
    pub mesh_id: MeshId,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    /// The route as written; `None` when the Mesh no longer has it.
    pub route: Option<String>,
    /// Whether the route already keeps a series (`+ Replaces`, either way).
    pub replaces: bool,
    pub sender: String,
    pub subject: String,
}

/// Group open Finds by Mesh, route and sender, then by subject with its
/// numbers set aside; a run of three or more under a route that keeps no
/// series is a suggestion, and a sender's runs side by side are one.
pub fn suggest_series(rows: &[OpenFindRow]) -> Vec<SeriesSuggestion> {
    type Runs<'a> = BTreeMap<String, Vec<&'a OpenFindRow>>;
    let mut groups: BTreeMap<(String, Option<String>, String), Runs<'_>> = BTreeMap::new();
    for r in rows {
        if r.replaces {
            continue;
        }
        let sender = r.sender.trim().to_ascii_lowercase();
        if sender.is_empty() {
            continue;
        }
        groups
            .entry((r.mesh_id.as_str().to_string(), r.route.clone(), sender))
            .or_default()
            .entry(series_subject(&r.subject))
            .or_default()
            .push(r);
    }
    let mut out = Vec::new();
    for ((mesh, route, sender), runs) in groups {
        let mut runs: Vec<Vec<&OpenFindRow>> = runs
            .into_values()
            .filter(|run| run.len() >= SUGGEST_AT)
            .collect();
        if runs.is_empty() {
            continue;
        }
        runs.sort_by_key(|run| std::cmp::Reverse(run.len()));
        let count: usize = runs.iter().map(Vec::len).sum();
        let first = runs[0][0];
        let sample = first.subject.trim().to_string();
        let what = match &first.mesh_name {
            Some(n) => format!("the {n} Mesh"),
            None => "this Mesh".to_string(),
        };
        let route_text = route
            .as_deref()
            .map(|r| format!(" route `{r}`"))
            .unwrap_or_default();
        let side = if runs.len() > 1 {
            format!(", in {} runs side by side", runs.len())
        } else {
            String::new()
        };
        let text = format!(
            "{count} open Finds from {sender} say the same thing, numbers aside (\"{sample}\"){side}, under {what}'s{route_text}. I can propose `+ Replaces: same subject` on that route: the newest of each run stays open and the rest close as Superseded; you would see the rule text and a simulation before anything changes."
        );
        out.push(SeriesSuggestion {
            mesh_id: MeshId::from(mesh.as_str()),
            mesh_version: first.mesh_version,
            mesh_name: first.mesh_name.clone(),
            route,
            sender,
            count: count as u32,
            runs: runs.len() as u32,
            sample,
            text,
            find_ids: runs.iter().flatten().map(|r| r.find_id.clone()).collect(),
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.count));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(find: &str, sender: &str, subject: &str, replaces: bool) -> OpenFindRow {
        OpenFindRow {
            find_id: FindId::from(find),
            mesh_id: MeshId::from("mesh_notices"),
            mesh_version: Some(3),
            mesh_name: Some("Account notices".into()),
            route: Some("ELSE IF account_notification@v1 > .85 → Find: Do".into()),
            replaces,
            sender: sender.into(),
            subject: subject.into(),
        }
    }

    #[test]
    fn three_open_finds_on_one_subject_from_one_sender_yield_a_suggestion() {
        let low = "Your August Smart Lock batteries are critically low";
        let rows = [
            row("f1", "noreply@august.com", low, false),
            row("f2", "noreply@august.com", low, false),
            row("f3", "NoReply@august.com ", low, false),
            row(
                "f4",
                "sc-noreply@google.com",
                "New reasons prevent pages from being indexed",
                false,
            ),
        ];
        let s = suggest_series(&rows);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!((s[0].count, s[0].runs), (3, 1));
        assert_eq!(s[0].sender, "noreply@august.com");
        assert_eq!(s[0].find_ids.len(), 3);
        assert!(
            s[0].text.contains("`+ Replaces: same subject`"),
            "{}",
            s[0].text
        );
        assert!(
            s[0].text.contains("the Account notices Mesh"),
            "{}",
            s[0].text
        );
    }

    #[test]
    fn runs_differ_by_subject_past_its_numbers_and_count_side_by_side() {
        let a = |f, s| row(f, "getsupport@moniker.com", s, false);
        let rows = [
            a("1", "a.com Expires in 28 days"),
            a("2", "Re: a.com Expires in 14 days"),
            a("3", "a.com Expires in 7 days"),
            a("4", "b.com Expires in 28 days"),
            a("5", "b.com Expires in 14 days"),
            a("6", "b.com Expires in 7 days"),
            a("7", "4503261 Re: DNS Nameservers"),
        ];
        let s = suggest_series(&rows);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!((s[0].count, s[0].runs), (6, 2));
        assert!(
            s[0].text.contains("in 2 runs side by side"),
            "{}",
            s[0].text
        );
    }

    #[test]
    fn a_route_that_keeps_a_series_and_a_short_run_suggest_nothing() {
        let kept: Vec<_> = (0..4)
            .map(|i| row(&format!("k{i}"), "x@y.com", "same thing", true))
            .collect();
        assert!(suggest_series(&kept).is_empty());
        let short = [
            row("s1", "x@y.com", "same thing", false),
            row("s2", "x@y.com", "same thing", false),
        ];
        assert!(suggest_series(&short).is_empty());
    }
}
