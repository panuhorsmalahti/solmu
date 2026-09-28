use super::*;
use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::post,
};

#[derive(Clone)]
struct Fixture {
    workspace: std::path::PathBuf,
    legacy: bool,
    sse: bool,
}
async fn rpc(
    State(state): State<Fixture>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Response {
    solmu_e2e::support::mcp::record(&state.workspace, &request);
    assert_eq!(headers.get("authorization").unwrap(), "Bearer fixture-key");
    if !state.legacy {
        assert_eq!(headers.get("mcp-protocol-version").unwrap(), "2026-07-28");
        assert_eq!(
            headers.get("mcp-method").unwrap().to_str().unwrap(),
            request["method"].as_str().unwrap()
        );
        assert_eq!(
            request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
            "2026-07-28"
        );
        assert!(headers.get("mcp-session-id").is_none());
        if request["method"] == "tools/call" {
            assert_eq!(headers.get("mcp-name").unwrap(), "search");
            assert_eq!(headers.get("mcp-param-query").unwrap(), "Architecture");
        }
    }
    let Some(response) = solmu_e2e::support::mcp::respond(&request, &state.workspace, state.legacy)
    else {
        return StatusCode::ACCEPTED.into_response();
    };
    if state.legacy && request["method"] == "server/discover" {
        return (StatusCode::BAD_REQUEST, Json(response)).into_response();
    }
    let mut response = if state.sse {
        Sse::new(futures_util::stream::once(async move {
            Ok::<_, std::convert::Infallible>(
                Event::default()
                    .event("message")
                    .json_data(response)
                    .unwrap(),
            )
        }))
        .into_response()
    } else {
        Json(response).into_response()
    };
    if state.legacy && request["method"] == "initialize" {
        response
            .headers_mut()
            .insert("mcp-session-id", "fixture-session".parse().unwrap());
    }
    response
}
#[tokio::test]
async fn streamable_http_supports_json_and_sse_responses_modern_headers_and_legacy_sessions() {
    for (legacy, sse) in [(false, false), (false, true), (true, false), (true, true)] {
        let backend = Backend::start().await;
        let thread = backend.create_thread("HTTP MCP").await;
        let id = thread["id"].as_str().unwrap();
        let workspace = std::path::PathBuf::from(thread["workspace"].as_str().unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        let app = Router::new().route("/mcp", post(rpc)).with_state(Fixture {
            workspace: workspace.clone(),
            legacy,
            sse,
        });
        let server = tokio::spawn(async {
            axum::serve(listener, app).await.unwrap();
        });
        std::fs::write(workspace.join(".mcp.json"),json!({"mcpServers":{"remote":{"type":"http","url":url,"headers":{"Authorization":"Bearer fixture-key"}}}}).to_string()).unwrap();
        let status = super::mcp::catalog(&backend, id).await;
        assert_eq!(
            status["servers"][0]["status"], "connected",
            "legacy={legacy} sse={sse}: {status}"
        );
        assert!(!status.to_string().contains("fixture-key"));
        let message = backend.send_message(id,&format!("TOOLS {}",json!([{"name":super::mcp::tool(&status,"search"),"arguments":{"query":"Architecture"}}]))).await;
        assert!(
            complete_reply(&backend, id, &message)
                .await
                .contains(&"done".into())
        );
        let runs: Value = backend
            .client
            .get(backend.endpoint(&format!("/api/v1/threads/{id}/tools")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(runs["items"][0]["status"], "completed", "{runs}");
        assert_eq!(
            runs["items"][0]["result"]["output"]["structuredContent"]["query"],
            "Architecture"
        );
        server.abort();
    }
}
