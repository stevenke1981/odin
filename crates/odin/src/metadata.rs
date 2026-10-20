use serde_json::{Value, json};

pub fn build_metadata() -> Value {
    let dirty = match env!("ODIN_BUILD_DIRTY") {
        "true" => json!(true),
        "false" => json!(false),
        _ => Value::Null,
    };
    json!({
        "name": "Odin", "version": env!("CARGO_PKG_VERSION"),
        "revision": env!("ODIN_BUILD_REVISION"), "tracked_source_dirty": dirty,
        "target": env!("ODIN_BUILD_TARGET"), "profile": env!("ODIN_BUILD_PROFILE"),
        "compiler": env!("ODIN_BUILD_COMPILER"), "standalone": true,
        "external_tool_bridge": false, "model_token_usage": "UNKNOWN",
        "schema_version": 1
    })
}
