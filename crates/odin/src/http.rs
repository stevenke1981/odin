use crate::mcp::OdinMcp;
use anyhow::{Result,bail};
use axum::{Router,body::{Body,to_bytes},extract::{Path,Request,State},http::{StatusCode,header},middleware::{self,Next},response::{Html,IntoResponse,Response},routing::get,Json};
use odin_engine::Engine;
use rmcp::transport::streamable_http_server::{StreamableHttpService,StreamableHttpServerConfig,session::local::LocalSessionManager};
use serde_json::json;
use std::{net::SocketAddr,sync::Arc,time::Duration};

#[derive(Clone)]struct Auth{token:Arc<String>,hosts:Arc<Vec<String>>,origins:Arc<Vec<String>>}
pub async fn serve(engine:Arc<Engine>,address:SocketAddr,token:String)->Result<()> {
    if !address.ip().is_loopback(){bail!("This release binds only loopback. Remote deployment requires a separately reviewed authenticated gateway.");}
    if token.len()<32||token.len()>512||!token.bytes().all(|b|b.is_ascii_graphic()){
        bail!("HTTP bearer token must contain 32..512 visible ASCII bytes");
    }
    let listener=tokio::net::TcpListener::bind(address).await?;let address=listener.local_addr()?;
    let host=address.to_string();let localhost=format!("localhost:{}",address.port());
    let auth=Auth{token:Arc::new(token),hosts:Arc::new(vec![host.clone(),localhost.clone()]),
        origins:Arc::new(vec![format!("http://{host}"),format!("http://{localhost}")])};
    let cancel=tokio_util::sync::CancellationToken::new();let service_engine=engine.clone();
    let mcp=StreamableHttpService::new(move||Ok(OdinMcp{engine:service_engine.clone()}),
        LocalSessionManager::default().into(),StreamableHttpServerConfig::default().with_cancellation_token(cancel.child_token()));
    let projects_engine=engine.clone();let audit_engine=engine.clone();
    let protected=Router::new().nest_service("/mcp",mcp)
        .route("/api/projects",get(move||{let engine=projects_engine.clone();async move{Json(engine.project_summaries())}}))
        .route("/api/audit/{project}",get(move|Path(project):Path<String>|{let engine=audit_engine.clone();async move{
            match engine.project(&project).and_then(|p|p.store.recent(100)){
                Ok(value)=>(StatusCode::OK,Json(value)),Err(error)=>(StatusCode::BAD_REQUEST,Json(json!({"error":error}))),
            }
        }}))
        .route_layer(middleware::from_fn_with_state(auth,authorize));
    let router=Router::new().route("/",get(||async{Html(include_str!("../../../dashboard/index.html"))})).merge(protected);
    eprintln!("Odin HTTP on http://{address}; /mcp and /api require the configured bearer token");
    axum::serve(listener,router).with_graceful_shutdown(async move{let _=tokio::signal::ctrl_c().await;cancel.cancel();}).await?;
    Ok(())
}
async fn authorize(State(auth):State<Auth>,request:Request,next:Next)->Response{
    let host=request.headers().get(header::HOST).and_then(|v|v.to_str().ok()).unwrap_or("");
    if !auth.hosts.iter().any(|h|h==host){return (StatusCode::FORBIDDEN,"Unexpected Host").into_response();}
    if let Some(origin)=request.headers().get(header::ORIGIN){
        if !origin.to_str().ok().is_some_and(|o|auth.origins.iter().any(|a|a==o)){return (StatusCode::FORBIDDEN,"Cross-origin access denied").into_response();}
    }
    let supplied=request.headers().get(header::AUTHORIZATION).and_then(|v|v.to_str().ok()).and_then(|v|v.strip_prefix("Bearer ")).unwrap_or("");
    if !tokens_match(supplied.as_bytes(),auth.token.as_bytes()){
        return (StatusCode::UNAUTHORIZED,[(header::WWW_AUTHENTICATE,"Bearer realm=\"odin-local\"")],"Bearer token required").into_response();
    }
    let(parts,body)=request.into_parts();
    let body=match tokio::time::timeout(Duration::from_secs(10),to_bytes(body,8*1024*1024)).await{
        Ok(Ok(bytes))=>bytes,Ok(Err(_))=>return (StatusCode::PAYLOAD_TOO_LARGE,"Request too large").into_response(),
        Err(_)=>return (StatusCode::REQUEST_TIMEOUT,"Request body timed out").into_response(),
    };
    let mut response=next.run(Request::from_parts(parts,Body::from(body))).await;
    response.headers_mut().insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));
    response
}
fn tokens_match(a:&[u8],b:&[u8])->bool{
    if a.len()!=b.len(){return false;}let mut difference=0u8;for(x,y)in a.iter().zip(b){difference|=x^y;}difference==0
}
#[cfg(test)]mod tests{use super::*;#[test]fn tokens_are_compared_exactly(){assert!(tokens_match(b"abc",b"abc"));assert!(!tokens_match(b"abc",b"abd"));assert!(!tokens_match(b"a",b"aa"));}}
