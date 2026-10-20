//! Validator for the explicit JSON-Schema subset used in tools.json.
//! Not advertised as a general-purpose JSON-Schema implementation.
use odin_core::{Fault,Result};
use serde_json::Value;
pub fn validate(schema:&Value,value:&Value)->Result<()> {walk(schema,value,"$",0)}
fn walk(schema:&Value,value:&Value,path:&str,depth:usize)->Result<()> {
    if depth>32{return Err(Fault::invalid("Arguments exceed nesting limit"));}
    let check=|kind:&str|match kind{"object"=>value.is_object(),"array"=>value.is_array(),"string"=>value.is_string(),"integer"=>value.is_i64()||value.is_u64(),"boolean"=>value.is_boolean(),"null"=>value.is_null(),"number"=>value.is_number(),_=>false};
    if let Some(t)=schema.get("type"){
        let valid=if let Some(s)=t.as_str(){check(s)}else if let Some(a)=t.as_array(){a.iter().filter_map(Value::as_str).any(check)}else{false};
        if !valid{return Err(Fault::invalid(format!("Wrong type at {path}")));}
    }
    if let Some(values)=schema.get("enum").and_then(Value::as_array){if !values.contains(value){return Err(Fault::invalid(format!("Invalid enum value at {path}")));}}
    if let Some(n)=value.as_f64(){
        if schema.get("minimum").and_then(Value::as_f64).is_some_and(|min|n<min)||schema.get("maximum").and_then(Value::as_f64).is_some_and(|max|n>max){return Err(Fault::invalid(format!("Number out of range at {path}")));}
    }
    if let Some(object)=value.as_object(){
        if let Some(required)=schema.get("required").and_then(Value::as_array){for key in required.iter().filter_map(Value::as_str){if !object.contains_key(key){return Err(Fault::invalid(format!("Missing {path}.{key}")));}}}
        for(key,item)in object{
            if let Some(child)=schema.get("properties").and_then(|p|p.get(key)){walk(child,item,&format!("{path}.{key}"),depth+1)?;}
            else if schema.get("additionalProperties")==Some(&Value::Bool(false)){return Err(Fault::invalid(format!("Unknown argument {path}.{key}")));}
        }
    }
    if let Some(items)=value.as_array(){
        if schema.get("maxItems").and_then(Value::as_u64).is_some_and(|max|items.len() as u64>max){return Err(Fault::invalid(format!("Too many array items at {path}")));}
        if let Some(child)=schema.get("items"){for(i,item)in items.iter().enumerate(){walk(child,item,&format!("{path}[{i}]"),depth+1)?;}}
    }
    Ok(())
}
#[cfg(test)]mod tests{
    use super::*;use serde_json::json;
    #[test]fn rejects_unknown_fields(){assert!(validate(&json!({"type":"object","properties":{},"additionalProperties":false}),&json!({"x":1})).is_err());}
    #[test]fn accepts_nullable(){assert!(validate(&json!({"type":["null","string"]}),&Value::Null).is_ok());}
    #[test]fn rejects_missing(){assert!(validate(&json!({"type":"object","required":["p"]}),&json!({})).is_err());}
}
