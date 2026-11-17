//! Runs the reference table (`tests/cases/refs.json`) against the collector.

use std::collections::HashMap;

use op_reason_core::refs;
use serde_json::{Value, json};

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|s| s.as_str().expect("string").to_string())
        .collect()
}

fn table() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cases/refs.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn reference_table() {
    let table = table();
    let cases = table["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "the table should stay comprehensive");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let args = strings(&case["args"]);
        let files: HashMap<String, String> = case
            .get("files")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let read = |path: &str| files.get(path).cloned();
        let stdin = case["stdin"].as_str();
        let plan = refs::plan(&args);
        let got = refs::collect(&args, &read, stdin.filter(|_| plan.stdin));
        let expect = &case["expect"];
        assert_eq!(got.list, strings(&expect["list"]), "{name}: list");
        let from: Vec<Value> = got
            .sources()
            .into_iter()
            .map(|s| serde_json::to_value(s).unwrap())
            .collect();
        assert_eq!(Value::Array(from), expect["from"], "{name}: from");
        assert_eq!(
            got.account.as_deref(),
            expect["account"].as_str(),
            "{name}: account"
        );
    }
}

#[test]
fn link_table() {
    let table = table();
    let links = table["links"].as_array().unwrap();
    assert!(links.len() >= 8);
    for l in links {
        let r = l["ref"].as_str().unwrap();
        let got = refs::link_ids(r).map(|(v, i)| json!([v, i]));
        let want = l["vault"]
            .as_str()
            .map(|v| json!([v, l["item"].as_str().unwrap()]));
        assert_eq!(got, want, "{r}");
    }
}
