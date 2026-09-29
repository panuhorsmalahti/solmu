use super::*;
use ring::hmac;
use std::time::Duration;

#[tokio::test]
async fn webhook_crud_authentication_delivery_deduplication_and_agent_response() {
    let backend = Backend::start().await;
    let created = backend.client.post(backend.endpoint("/api/v1/webhooks"))
        .json(&json!({"name":"GitHub","auth_type":"github-hmac-sha256","secret":"github-test-secret-with-entropy","instructions":"Summarize the event and suggest next steps."}))
        .send().await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let hook: Value = created.json().await.unwrap();
    let id = hook["id"].as_str().unwrap();
    assert_eq!(hook["enabled"], false);
    assert_eq!(hook["secret_configured"], true);
    assert_eq!(
        hook["port"],
        backend
            .webhook_url
            .rsplit_once(':')
            .unwrap()
            .1
            .parse::<u16>()
            .unwrap()
    );
    assert!(
        serde_json::to_string(&hook)
            .unwrap()
            .find("github-test-secret")
            .is_none()
    );

    let listed: Value = backend
        .client
        .get(backend.endpoint("/api/v1/webhooks"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed[0]["id"], id);
    let endpoint = format!("{}/hooks/{id}", backend.webhook_url);
    let body = json!({"action":"opened","issue":{"title":"Improve the README"}}).to_string();

    let unauthorized = backend
        .client
        .post(&endpoint)
        .body(body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(
        unauthorized.status(),
        StatusCode::NOT_FOUND,
        "disabled hooks are not active"
    );
    let enabled = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/webhooks/{id}")))
        .json(&json!({"enabled":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);

    let bad_signature = backend
        .client
        .post(&endpoint)
        .header(
            "x-hub-signature-256",
            "sha256=0000000000000000000000000000000000000000000000000000000000000000",
        )
        .body(body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(bad_signature.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        0
    );

    let key = hmac::Key::new(hmac::HMAC_SHA256, b"github-test-secret-with-entropy");
    let signature = format!(
        "sha256={}",
        hex::encode(hmac::sign(&key, body.as_bytes()).as_ref())
    );
    let first = backend
        .client
        .post(&endpoint)
        .header("x-hub-signature-256", signature.clone())
        .header("x-github-event", "issues")
        .header("x-github-delivery", "delivery-1")
        .header("content-type", "application/json")
        .body(body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let accepted: Value = first.json().await.unwrap();
    let thread = accepted["thread_id"].as_str().unwrap();
    assert_eq!(backend.messages(thread).await["items"][0]["role"], "user");
    assert!(
        backend.messages(thread).await["items"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Improve the README")
    );
    assert!(
        backend.messages(thread).await["items"][0]["content"]
            .as_str()
            .unwrap()
            .contains("Summarize the event")
    );

    let duplicate = backend
        .client
        .post(&endpoint)
        .header("x-hub-signature-256", signature)
        .header("x-github-event", "issues")
        .header("x-github-delivery", "delivery-1")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::ACCEPTED);
    assert_eq!(duplicate.json::<Value>().await.unwrap()["duplicate"], true);
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );

    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if backend.messages(thread).await["items"]
                .as_array()
                .unwrap()
                .len()
                == 2
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        backend.messages(thread).await["items"][1]["role"],
        "assistant"
    );
    assert_eq!(backend.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn bearer_webhooks_validate_configuration_and_can_be_disabled_or_deleted() {
    let backend = Backend::start().await;
    let invalid = backend
        .client
        .post(backend.endpoint("/api/v1/webhooks"))
        .json(&json!({"name":"Short key","auth_type":"bearer","secret":"short"}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let created = backend.client.post(backend.endpoint("/api/v1/webhooks"))
        .json(&json!({"name":"Build trigger","auth_type":"bearer","secret":"long-enough-bearer-token"})).send().await.unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let hook: Value = created.json().await.unwrap();
    let id = hook["id"].as_str().unwrap();
    let url = format!("{}/hooks/{id}", backend.webhook_url);
    let inactive = backend
        .client
        .post(&url)
        .bearer_auth("long-enough-bearer-token")
        .json(&json!({"ref":"main"}))
        .send()
        .await
        .unwrap();
    assert_eq!(inactive.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        backend
            .client
            .patch(backend.endpoint(&format!("/api/v1/webhooks/{id}")))
            .json(&json!({"enabled":true}))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let wrong_token = backend
        .client
        .post(&url)
        .bearer_auth("wrong-bearer-token-value")
        .json(&json!({"ref":"main"}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_token.status(), StatusCode::UNAUTHORIZED);
    let active = backend
        .client
        .post(&url)
        .bearer_auth("long-enough-bearer-token")
        .header("idempotency-key", "build-1")
        .json(&json!({"ref":"main"}))
        .send()
        .await
        .unwrap();
    assert_eq!(active.status(), StatusCode::ACCEPTED);
    let thread = active.json::<Value>().await.unwrap()["thread_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let disabled = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/webhooks/{id}")))
        .json(&json!({"enabled":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
    let stopped = backend
        .client
        .post(&url)
        .bearer_auth("long-enough-bearer-token")
        .json(&json!({"ref":"main"}))
        .send()
        .await
        .unwrap();
    assert_eq!(stopped.status(), StatusCode::NOT_FOUND);
    assert!(
        !backend.messages(&thread).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        backend
            .client
            .delete(backend.endpoint(&format!("/api/v1/webhooks/{id}")))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        backend
            .client
            .get(backend.endpoint("/api/v1/webhooks"))
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
