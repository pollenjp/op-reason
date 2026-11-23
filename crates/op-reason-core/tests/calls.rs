//! Runs the call table (`tests/cases/calls.json`) against the planner.

use std::collections::HashMap;
use std::ffi::OsString;

use op_reason_core::args::{self, Name, Plan};
use serde_json::Value;

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|s| s.as_str().expect("string").to_string())
        .collect()
}

#[test]
fn call_table() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/cases/calls.json");
    let table: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let cases = table["cases"].as_array().unwrap();
    assert!(cases.len() >= 30, "the table should stay comprehensive");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let invoked = match case["invokedAs"].as_str().unwrap() {
            "op-reason" => Name::OpReason,
            "op" => Name::Other,
            other => panic!("{name}: unknown invokedAs {other}"),
        };
        let argv: Vec<OsString> = strings(&case["argv"])
            .into_iter()
            .map(OsString::from)
            .collect();
        let env: HashMap<String, String> = case
            .get("env")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let lookup = |k: &str| env.get(k).cloned();
        let (parsed, plan) = args::plan(invoked, &argv, &lookup);
        let expect = &case["expect"];
        let action = match &plan {
            Plan::Usage => "usage",
            Plan::Version => "version",
            Plan::Skill => "skill",
            Plan::Completion { .. } => "completion",
            Plan::UsageError(_) => "error",
            Plan::Log(_) => "log",
            Plan::Settings(_) => "settings",
            Plan::Refuse(_) => "refuse",
            Plan::Run { .. } => "run",
        };
        assert_eq!(
            action,
            expect["action"].as_str().unwrap(),
            "{name}: action ({plan:?})"
        );
        if let Some(why) = expect.get("why") {
            let Plan::Refuse(r) = &plan else {
                panic!("{name}: not a refusal")
            };
            assert_eq!(r.why(), why.as_str().unwrap(), "{name}: why");
        }
        if let Some(exit) = expect.get("exit") {
            let code = match &plan {
                Plan::Refuse(_) => args::EXIT_NO_REASON,
                Plan::UsageError(_) => 2,
                _ => 0,
            };
            assert_eq!(i64::from(code), exit.as_i64().unwrap(), "{name}: exit");
        }
        if let Some(mode) = expect.get("mode") {
            let Plan::Run { mode: m, .. } = &plan else {
                panic!("{name}: not a run")
            };
            assert_eq!(m.as_str(), mode.as_str().unwrap(), "{name}: mode");
        }
        if let Some(kind) = expect.get("kind") {
            let Plan::Run { kind: k, .. } = &plan else {
                panic!("{name}: not a run")
            };
            assert_eq!(k.as_str(), kind.as_str().unwrap(), "{name}: kind");
        }
        if let Some(op_args) = expect.get("opArgs") {
            assert_eq!(parsed.op_args_text(), strings(op_args), "{name}: opArgs");
        }
        let masked = args::mask(&parsed.op_args_text());
        if let Some(want) = expect.get("masked") {
            assert_eq!(masked, strings(want), "{name}: masked");
        }
        if let Some(want) = expect.get("command") {
            assert_eq!(
                args::display(&masked),
                want.as_str().unwrap(),
                "{name}: command"
            );
        }
        if let Some(want) = expect.get("reason") {
            assert_eq!(parsed.reason, want.as_str().unwrap(), "{name}: reason");
        }
        if let Some(want) = expect.get("shell") {
            let Plan::Completion { shell, .. } = &plan else {
                panic!("{name}: not completion")
            };
            assert_eq!(shell, want.as_str().unwrap(), "{name}: shell");
        }
        if let Some(want) = expect.get("completionCommand") {
            let Plan::Completion { command, .. } = &plan else {
                panic!("{name}: not completion")
            };
            assert_eq!(
                command,
                want.as_str().unwrap(),
                "{name}: completion command"
            );
        }
        if let Some(want) = expect.get("logArgs") {
            let Plan::Log(rest) = &plan else {
                panic!("{name}: not log")
            };
            let rest: Vec<String> = rest
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect();
            assert_eq!(rest, strings(want), "{name}: log args");
        }
    }
}
