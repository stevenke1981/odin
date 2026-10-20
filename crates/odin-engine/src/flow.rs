//! A deliberately non-Turing-complete, read-only JSON dataflow interface.
//! References resolve JSON values, not executable source text.
use crate::{Engine,string,number};
use odin_core::{Fault,Result,config::validate_id};
use serde_json::{Value,json};
use std::{collections::BTreeMap,sync::Arc};

pub async fn run(engine:&Arc<Engine>,project:&str,args:&Value)->Result<Value>{
    let session=string(args,"session")?;validate_id(session)?;
    let key=(project.to_string(),session.to_string());
    let steps=args["steps"].as_array().ok_or_else(||Fault::invalid("steps must be an array"))?;
    if steps.len()>engine.config.limits.max_flow_steps{return Err(Fault::new("LIMIT_EXCEEDED","Too many flow steps"));}
    // The namespace is committed only after the entire read-only pipeline succeeds.
    let mut sessions=engine.flows.lock().await;
    if !sessions.contains_key(&key)&&sessions.len()>=engine.config.limits.max_flow_sessions{return Err(Fault::new("LIMIT_EXCEEDED","Flow session limit reached; clear an old session"));}
    let mut vars=sessions.get(&key).cloned().unwrap_or_default();let mut last=Value::Null;
    for step in steps{
        let name=string(step,"as")?;validate_id(name)?;
        let mut value=if let Some(tool)=step.get("tool").and_then(Value::as_str){
            if step.get("select").is_some(){return Err(Fault::invalid("A step cannot contain both tool and select"));}
            if !engine.read_tool(tool){return Err(Fault::denied("Flows can call nonrecursive native read tools only"));}
            let arguments=resolve(step.get("arguments").unwrap_or(&json!({})),&vars,0)?;
            engine.invoke(project.into(),tool.into(),arguments).await?
        }else if let Some(select)=step.get("select"){resolve(select,&vars,0)?}
        else{return Err(Fault::invalid("A step requires tool or select"));};
        if let Some(pointer)=step.get("pointer").and_then(Value::as_str){value=value.pointer(pointer).cloned().ok_or_else(||Fault::invalid("Step pointer does not exist"))?;}
        if step.get("take").is_some(){
            let take=number(step,"take",0) as usize;
            let array=value.as_array_mut().ok_or_else(||Fault::invalid("take requires an array"))?;array.truncate(take);
        }
        last=value.clone();vars.insert(name.into(),value);
        if vars.len()>128||serde_json::to_vec(&vars)?.len()>engine.config.limits.max_session_bytes{
            return Err(Fault::new("LIMIT_EXCEEDED","Flow namespace exceeds limit; select only required fields"));
        }
    }
    let result=if let Some(ret)=args.get("return"){resolve(ret,&vars,0)?}else{last};
    sessions.insert(key,vars);
    Ok(json!({"session":session,"steps_executed":steps.len(),"result":result,"language":"odin-json-flow","python_compatible":false}))
}
fn resolve(value:&Value,vars:&BTreeMap<String,Value>,depth:usize)->Result<Value>{
    if depth>24{return Err(Fault::invalid("Reference tree too deep"));}
    match value{
        Value::Object(object) if object.len()==1&&object.contains_key("$ref")=>{
            let reference=object["$ref"].as_str().ok_or_else(||Fault::invalid("$ref must be a string"))?;
            let(name,pointer)=reference.split_once('#').unwrap_or((reference,""));validate_id(name)?;
            vars.get(name).and_then(|v|v.pointer(pointer)).cloned().ok_or_else(||Fault::invalid(format!("Unresolved data reference: {reference}")))
        },
        Value::Object(object)=>{
            let mut result=serde_json::Map::new();for(key,value)in object{result.insert(key.clone(),resolve(value,vars,depth+1)?);}Ok(Value::Object(result))
        },
        Value::Array(values)=>Ok(Value::Array(values.iter().map(|v|resolve(v,vars,depth+1)).collect::<Result<_>>()?)),
        other=>Ok(other.clone()),
    }
}
pub async fn vars(engine:&Arc<Engine>,project:&str,session:&str)->Result<Value>{
    validate_id(session)?;let sessions=engine.flows.lock().await;
    let values=sessions.get(&(project.into(),session.into())).map(|vars|vars.iter().map(|(name,value)|
        json!({"name":name,"bytes":serde_json::to_vec(value).map(|v|v.len()).unwrap_or(0)})).collect::<Vec<_>>()).unwrap_or_default();
    Ok(json!({"session":session,"variables":values,"scope":project}))
}
pub async fn clear(engine:&Arc<Engine>,project:&str,session:&str)->Result<Value>{
    validate_id(session)?;let removed=engine.flows.lock().await.remove(&(project.into(),session.into())).is_some();
    Ok(json!({"cleared":removed,"session":session,"scope":project}))
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn refs_select_values_not_code(){let vars=BTreeMap::from([("x".into(),json!({"items":[1,2]}))]);assert_eq!(resolve(&json!({"$ref":"x#/items/1"}),&vars,0).unwrap(),2);}
    #[test]fn missing_reference_is_an_error(){assert!(resolve(&json!({"$ref":"missing"}),&BTreeMap::new(),0).is_err());}
    #[test]fn source_strings_are_inert(){assert_eq!(resolve(&json!("__import__('os').system('echo nope')"),&BTreeMap::new(),0).unwrap(),json!("__import__('os').system('echo nope')"));}
}
