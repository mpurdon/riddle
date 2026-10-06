//! The shared grammar corpus (`grammar.json`) against the Rust parser. The
//! browser mock runs the same file (`apps/desktop/src/lib/mocks/
//! dsl.grammar.test.ts`); before it existed, the mock's copy of the rules
//! for new route outputs was kept in step by hand and nothing noticed when
//! it was not.

use serde_json::Value;

/// Every field `expected` names has the same value in `actual`.
fn assert_subset(expected: &Value, actual: &Value, at: &str) {
    for (k, want) in expected.as_object().expect("expectations are objects") {
        let got = actual.get(k).unwrap_or(&Value::Null);
        assert_eq!(got, want, "{at}: field {k}");
    }
}

#[test]
fn every_case_in_the_shared_grammar_corpus_parses_as_written() {
    let corpus: Value = serde_json::from_str(include_str!("grammar.json")).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let result = riddle_mesh::parse::parse(case["text"].as_str().unwrap());
        if let Some(error) = case["error"].as_str() {
            let e = result.expect_err(name);
            assert!(e.message.contains(error), "{name}: {}", e.message);
            continue;
        }
        let mesh = serde_json::to_value(result.unwrap_or_else(|e| panic!("{name}: {}", e.message)))
            .unwrap();
        if let Some(expected) = case.get("mesh") {
            assert_subset(expected, &mesh, name);
        }
        if let Some(outputs) = case.get("outputs").and_then(Value::as_array) {
            let routes = mesh["routes"].as_array().unwrap();
            assert_eq!(routes.len(), outputs.len(), "{name}: route count");
            for (i, (want, route)) in outputs.iter().zip(routes).enumerate() {
                assert_subset(want, &route["output"], &format!("{name}, route {i}"));
            }
        }
    }
}
