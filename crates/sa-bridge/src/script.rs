//! Adapter from wire-protocol script commands to the sa-sdk executor.

use proto::{ScriptArg, ScriptCommand};
use sa_sdk::script::{self, Arg, Command};
use serde_json::{json, Map, Value};

/// # Safety
/// Game thread only.
pub unsafe fn run(cmds: &[ScriptCommand]) -> Result<Value, String> {
    let cmds: Vec<Command> = cmds
        .iter()
        .map(|c| Command {
            op: c.op,
            args: c
                .args
                .iter()
                .map(|a| match a {
                    ScriptArg::Int(v) => Arg::Int(*v),
                    ScriptArg::Float(v) => Arg::Float(*v),
                    ScriptArg::Var(i) => Arg::Var(*i),
                    ScriptArg::Str(s) => Arg::Str(s.clone()),
                })
                .collect(),
        })
        .collect();
    let out = script::run(&cmds)?;
    let results: Vec<Value> =
        out.conds.iter().map(|(op, cond)| json!({ "op": format!("{op:04X}"), "cond": cond })).collect();
    let mut vars = Map::new();
    for (i, raw) in out.vars {
        vars.insert(i.to_string(), json!({ "int": raw as i32, "float": f32::from_bits(raw) }));
    }
    Ok(json!({ "results": results, "vars": vars }))
}
