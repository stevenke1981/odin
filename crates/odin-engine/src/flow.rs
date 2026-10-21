//! A deliberately non-Turing-complete, read-only JSON dataflow interface.
//! References resolve JSON values, not executable source text.
use crate::{Engine, string};
use odin_core::{Fault, Result, config::validate_id};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

pub async fn run(engine: &Arc<Engine>, project: &str, args: &Value) -> Result<Value> {
    let session = string(args, "session")?;
    validate_id(session)?;
    let key = (project.to_string(), session.to_string());
    let steps = args["steps"]
        .as_array()
        .ok_or_else(|| Fault::invalid("steps must be an array"))?;
    if steps.len() > engine.config.limits.max_flow_steps {
        return Err(Fault::new("LIMIT_EXCEEDED", "Too many flow steps"));
    }
    // The namespace is committed only after the entire read-only pipeline succeeds.
    let namespace = {
        let mut sessions = engine.flows.lock().await;
        if !sessions.contains_key(&key) && sessions.len() >= engine.config.limits.max_flow_sessions
        {
            return Err(Fault::new(
                "LIMIT_EXCEEDED",
                "Flow session limit reached; clear an old session",
            ));
        }
        sessions
            .entry(key)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(BTreeMap::new())))
            .clone()
    };
    // Only this session is serialized. Cancellation/error drops the draft.
    let mut committed = namespace.lock().await;
    let maximum = engine.config.limits.max_session_bytes;
    check_namespace(&committed, None, maximum)?;
    let mut vars = committed.clone();
    let mut last_name = None;
    for step in steps {
        let name = string(step, "as")?;
        validate_id(name)?;
        let mut value = if let Some(tool) = step.get("tool").and_then(Value::as_str) {
            if step.get("select").is_some() {
                return Err(Fault::invalid("A step cannot contain both tool and select"));
            }
            if !engine.read_tool(tool) {
                return Err(Fault::denied(
                    "Flows can call nonrecursive native read tools only",
                ));
            }
            let empty = json!({});
            let arguments = resolve(step.get("arguments").unwrap_or(&empty), &vars, maximum)?;
            engine
                .invoke(project.into(), tool.into(), arguments)
                .await?
        } else if let Some(select) = step.get("select") {
            resolve(select, &vars, maximum)?
        } else {
            return Err(Fault::invalid("A step requires tool or select"));
        };
        // Tool results are untrusted values too. Check them before pointer/map
        // can clone any subtree; narrowing does not bypass the input budget.
        Budget::new(maximum).consume(&value, 0)?;
        if let Some(pointer) = step.get("pointer").and_then(Value::as_str) {
            let selected = value
                .pointer(pointer)
                .ok_or_else(|| Fault::invalid("Step pointer does not exist"))?;
            value = bounded_clone(selected, maximum)?;
        }
        if step.get("take").is_some() {
            let take = crate::number_usize(step, "take", 0);
            let array = value
                .as_array_mut()
                .ok_or_else(|| Fault::invalid("take requires an array"))?;
            array.truncate(take);
        }
        if let Some(pointer) = step.get("map").and_then(Value::as_str) {
            let items = value
                .as_array()
                .ok_or_else(|| Fault::invalid("map requires an array"))?;
            if items.len() > 10000 {
                return Err(Fault::new(
                    "LIMIT_EXCEEDED",
                    "map input exceeds 10000 values",
                ));
            }
            let mut budget = Budget::new(maximum);
            budget.charge(2, 1)?;
            let mut mapped = Vec::new();
            for item in items {
                if !mapped.is_empty() {
                    budget.charge(1, 0)?;
                }
                let selected = item
                    .pointer(pointer)
                    .ok_or_else(|| Fault::invalid("map pointer does not exist"))?;
                budget.consume(selected, 1)?;
                mapped.push(selected.clone());
            }
            value = Value::Array(mapped);
        }
        if let Some(filter) = step.get("filter") {
            let pointer = string(filter, "pointer")?;
            let equals = filter
                .get("equals")
                .ok_or_else(|| Fault::invalid("filter requires equals"))?;
            let items = value
                .as_array_mut()
                .ok_or_else(|| Fault::invalid("filter requires an array"))?;
            if items.len() > 10000 {
                return Err(Fault::new(
                    "LIMIT_EXCEEDED",
                    "filter input exceeds 10000 values",
                ));
            }
            items.retain(|item| item.pointer(pointer) == Some(equals));
        }
        if let Some(reduce) = step.get("reduce").and_then(Value::as_str) {
            let items = value
                .as_array()
                .ok_or_else(|| Fault::invalid("reduce requires an array"))?;
            if items.len() > 10000 {
                return Err(Fault::new(
                    "LIMIT_EXCEEDED",
                    "reduce input exceeds 10000 values",
                ));
            }
            value = match reduce {
                "count" => json!(items.len()),
                "sum" => {
                    let sum = items.iter().try_fold(0.0f64, |total, item| {
                        item.as_f64()
                            .map(|n| total + n)
                            .filter(|n| n.is_finite())
                            .ok_or_else(|| Fault::invalid("sum requires finite numbers"))
                    })?;
                    json!(sum)
                }
                _ => return Err(Fault::invalid("Unknown reduce builtin")),
            };
        }
        // Include every retained variable, its key and structural overhead
        // before insertion. Avoid a second clone of every step as `last`.
        check_namespace(&vars, Some((name, &value)), maximum)?;
        vars.insert(name.into(), value);
        last_name = Some(name);
    }
    let result = if let Some(ret) = args.get("return") {
        resolve(ret, &vars, maximum)?
    } else if let Some(name) = last_name {
        bounded_clone(
            vars.get(name)
                .ok_or_else(|| Fault::new("CORRUPT_STATE", "Missing final flow value"))?,
            maximum,
        )?
    } else {
        Value::Null
    };
    *committed = vars;
    Ok(
        json!({"session":session,"steps_executed":steps.len(),"result":result,"language":"odin-json-flow","python_compatible":false}),
    )
}
const MAX_VALUE_NODES: usize = 65_536;
const MAX_VALUE_DEPTH: usize = 24;

struct Budget {
    bytes: usize,
    nodes: usize,
}
impl Budget {
    fn new(maximum: usize) -> Self {
        Self {
            bytes: maximum,
            nodes: maximum.min(MAX_VALUE_NODES),
        }
    }
    fn charge(&mut self, bytes: usize, nodes: usize) -> Result<()> {
        let remaining_bytes = self.bytes.checked_sub(bytes).ok_or_else(limit_error)?;
        let remaining_nodes = self.nodes.checked_sub(nodes).ok_or_else(limit_error)?;
        self.bytes = remaining_bytes;
        self.nodes = remaining_nodes;
        Ok(())
    }
    fn string(&mut self, text: &str) -> Result<()> {
        self.charge(2, 0)?;
        for byte in text.bytes() {
            let length = match byte {
                b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
                0..=0x1f => 6,
                _ => 1,
            };
            self.charge(length, 0)?;
        }
        Ok(())
    }
    // This walks borrowed data without allocating a serialized representation.
    // Charge before cloning a referenced subtree, never after allocation.
    fn consume(&mut self, value: &Value, depth: usize) -> Result<()> {
        if depth > MAX_VALUE_DEPTH {
            return Err(limit_error());
        }
        self.charge(0, 1)?;
        match value {
            Value::Null => self.charge(4, 0),
            Value::Bool(true) => self.charge(4, 0),
            Value::Bool(false) => self.charge(5, 0),
            Value::Number(number) => self.charge(number.to_string().len(), 0),
            Value::String(text) => self.string(text),
            Value::Array(items) => {
                self.charge(2, 0)?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        self.charge(1, 0)?;
                    }
                    self.consume(item, depth + 1)?;
                }
                Ok(())
            }
            Value::Object(items) => {
                self.charge(2, 0)?;
                for (index, (key, value)) in items.iter().enumerate() {
                    if index > 0 {
                        self.charge(1, 0)?;
                    }
                    self.key(key)?;
                    self.consume(value, depth + 1)?;
                }
                Ok(())
            }
        }
    }
    fn key(&mut self, key: &str) -> Result<()> {
        self.charge(1, 1)?;
        self.string(key)
    }
}
fn limit_error() -> Fault {
    Fault::new(
        "LIMIT_EXCEEDED",
        "Flow value exceeds byte, node or depth budget; narrow the input or references",
    )
}
fn bounded_clone(value: &Value, maximum: usize) -> Result<Value> {
    Budget::new(maximum).consume(value, 0)?;
    Ok(value.clone())
}
fn check_namespace(
    vars: &BTreeMap<String, Value>,
    replacement: Option<(&str, &Value)>,
    maximum: usize,
) -> Result<()> {
    let count =
        vars.len() + usize::from(replacement.is_some_and(|(name, _)| !vars.contains_key(name)));
    if count > 128 {
        return Err(limit_error());
    }
    let mut budget = Budget::new(maximum);
    budget.charge(2, 1)?;
    let mut nonempty = false;
    for (name, value) in vars
        .iter()
        .filter(|(name, _)| replacement.is_none_or(|(replace, _)| name.as_str() != replace))
    {
        if nonempty {
            budget.charge(1, 0)?;
        }
        nonempty = true;
        budget.key(name)?;
        budget.consume(value, 1)?;
    }
    if let Some((name, value)) = replacement {
        if nonempty {
            budget.charge(1, 0)?;
        }
        budget.key(name)?;
        budget.consume(value, 1)?;
    }
    Ok(())
}
fn resolve(value: &Value, vars: &BTreeMap<String, Value>, maximum: usize) -> Result<Value> {
    resolve_inner(value, vars, &mut Budget::new(maximum), 0)
}
fn resolve_inner(
    value: &Value,
    vars: &BTreeMap<String, Value>,
    budget: &mut Budget,
    depth: usize,
) -> Result<Value> {
    if depth > MAX_VALUE_DEPTH {
        return Err(limit_error());
    }
    match value {
        Value::Object(object) if object.len() == 1 && object.contains_key("$ref") => {
            let reference = object["$ref"]
                .as_str()
                .ok_or_else(|| Fault::invalid("$ref must be a string"))?;
            let (name, pointer) = reference.split_once('#').unwrap_or((reference, ""));
            validate_id(name)?;
            let selected = vars
                .get(name)
                .and_then(|v| v.pointer(pointer))
                .ok_or_else(|| Fault::invalid(format!("Unresolved data reference: {reference}")))?;
            budget.consume(selected, depth)?;
            Ok(selected.clone())
        }
        Value::Object(object) => {
            budget.charge(2, 1)?;
            let mut result = serde_json::Map::new();
            for (key, value) in object {
                if !result.is_empty() {
                    budget.charge(1, 0)?;
                }
                budget.key(key)?;
                result.insert(key.clone(), resolve_inner(value, vars, budget, depth + 1)?);
            }
            Ok(Value::Object(result))
        }
        Value::Array(values) => {
            budget.charge(2, 1)?;
            let mut output = Vec::new();
            for value in values {
                if !output.is_empty() {
                    budget.charge(1, 0)?;
                }
                output.push(resolve_inner(value, vars, budget, depth + 1)?);
            }
            Ok(Value::Array(output))
        }
        other => {
            budget.consume(other, depth)?;
            Ok(other.clone())
        }
    }
}
pub async fn vars(engine: &Arc<Engine>, project: &str, session: &str) -> Result<Value> {
    validate_id(session)?;
    let namespace = engine
        .flows
        .lock()
        .await
        .get(&(project.into(), session.into()))
        .cloned();
    let values = if let Some(namespace) = namespace {
        let vars = namespace.lock().await;
        vars.iter().map(|(name,value)|
        json!({"name":name,"bytes":serde_json::to_vec(value).map(|v|v.len()).unwrap_or(0)})).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    Ok(json!({"session":session,"variables":values,"scope":project}))
}
pub async fn clear(engine: &Arc<Engine>, project: &str, session: &str) -> Result<Value> {
    validate_id(session)?;
    let mut sessions = engine.flows.lock().await;
    let key = (project.into(), session.into());
    if let Some(namespace) = sessions.get(&key)
        && Arc::strong_count(namespace) > 1
    {
        return Err(Fault::conflict(
            "Flow session is in use; retry clear after its current operation finishes",
        ));
    }
    let removed = sessions.remove(&key).is_some();
    Ok(json!({"cleared":removed,"session":session,"scope":project}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refs_select_values_not_code() {
        let vars = BTreeMap::from([("x".into(), json!({"items":[1,2]}))]);
        assert_eq!(
            resolve(&json!({"$ref":"x#/items/1"}), &vars, 1024).unwrap(),
            2
        );
    }
    #[test]
    fn missing_reference_is_an_error() {
        assert!(resolve(&json!({"$ref":"missing"}), &BTreeMap::new(), 1024).is_err());
    }
    #[test]
    fn source_strings_are_inert() {
        assert_eq!(
            resolve(
                &json!("__import__('os').system('echo nope')"),
                &BTreeMap::new(),
                1024
            )
            .unwrap(),
            json!("__import__('os').system('echo nope')")
        );
    }
    #[tokio::test]
    async fn a_locked_session_does_not_lock_other_sessions_and_clear_rejects_busy() {
        let root = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let config = serde_json::from_value(
            json!({"state_dir":state.path(),"projects":{"one":{"root":root.path()}}}),
        )
        .unwrap();
        let engine = Engine::new(config).unwrap();
        let namespace = Arc::new(tokio::sync::Mutex::new(BTreeMap::new()));
        engine
            .flows
            .lock()
            .await
            .insert(("one".into(), "busy".into()), namespace.clone());
        let _guard = namespace.lock().await;
        let other = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            run(
                &engine,
                "one",
                &json!({"session":"other","steps":[{"as":"value","select":42}]}),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(other["result"], 42);
        assert_eq!(
            clear(&engine, "one", "busy").await.unwrap_err().code,
            "CONFLICT"
        );
        assert!(
            clear(&engine, "one", "other").await.unwrap()["cleared"]
                .as_bool()
                .unwrap()
        );
    }
    #[test]
    fn repeated_references_and_namespace_growth_fail_before_large_clones() {
        let vars = BTreeMap::from([("x".into(), json!("x".repeat(1024)))]);
        let references = json!([{"$ref":"x"},{"$ref":"x"},{"$ref":"x"}]);
        assert_eq!(
            resolve(&references, &vars, 1536).unwrap_err().code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(
            check_namespace(&vars, Some(("y", &json!("y".repeat(1024)))), 1536)
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(vars.len(), 1);
    }
    #[test]
    fn measurement_handles_escaping_and_node_limit_without_serialized_copy() {
        let value = json!({"繁體\"":"\n\u{0000}\\世界","array":[1,true,null]});
        let size = serde_json::to_vec(&value).unwrap().len();
        let mut budget = Budget::new(size);
        budget.consume(&value, 0).unwrap();
        assert_eq!(budget.bytes, 0);
        assert_eq!(
            bounded_clone(&json!(vec![0; MAX_VALUE_NODES + 1]), 1_048_576)
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
    }
}
