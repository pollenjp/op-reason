//! Runs the table of process chains (`tests/cases/roots.json`) against the rules that find the root of a call.

use op_reason_core::ssh::cmdline;
use op_reason_core::ssh::roots::{self, Node, Os};
use serde_json::Value;

fn node(os: Os, v: &Value) -> Node {
    let name = v["name"].as_str().expect("name").to_owned();
    let args = match os {
        Os::Windows => v["cmd"]
            .as_str()
            .map(cmdline::split_windows)
            .unwrap_or_default(),
        Os::Linux => v["args"]
            .as_array()
            .expect("args")
            .iter()
            .map(|a| a.as_str().expect("string").to_owned())
            .collect(),
    };
    Node { name, args }
}

#[test]
fn root_table() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cases/roots.json");
    let table: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let cases = table["cases"].as_array().unwrap();
    assert!(cases.len() >= 20, "the table should stay comprehensive");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let os = match case["os"].as_str().unwrap() {
            "windows" => Os::Windows,
            "linux" => Os::Linux,
            other => panic!("{name}: unknown os {other}"),
        };
        let chain: Vec<Node> = case["chain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| node(os, v))
            .collect();
        let found = roots::find(os, &chain);
        assert_eq!(
            found.rule.as_str(),
            case["rule"].as_str().unwrap(),
            "{name}: rule"
        );
        assert_eq!(
            found.root as u64,
            case["root"].as_u64().unwrap(),
            "{name}: root"
        );
        assert_eq!(
            found.shell.map(|s| s as u64),
            case["shell"].as_u64(),
            "{name}: shell"
        );
    }
}
