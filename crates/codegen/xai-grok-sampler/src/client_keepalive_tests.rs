use super::*;
use std::time::Duration;

use serde_json::json;
use xai_grok_test_support::{MockInferenceServer, ScriptedResponse, SseEvent};

use crate::events::{SamplingErrorKind, SamplingEvent};
use crate::stream::responses::stream_responses;
use crate::types::RequestId;

async fn response_stream(
    server: &MockInferenceServer,
) -> BoxStream<'static, Result<rs::ResponseStreamEvent>> {
    let client = SamplingClient::new(SamplerConfig {
        api_key: Some("test-key".into()),
        base_url: server.url(),
        model: "test-model".into(),
        api_backend: ApiBackend::Responses,
        ..Default::default()
    })
    .unwrap();
    client
        .create_response_stream(CreateResponseWrapper::new(rs::CreateResponse {
            input: rs::InputParam::Text("hi".into()),
            ..Default::default()
        }))
        .await
        .unwrap()
        .0
}

#[tokio::test]
async fn keepalive_preserves_text_tool_arguments_and_completion() {
    let server = MockInferenceServer::start().await.unwrap();
    let call = json!({
        "type": "function_call", "id": "fc_1", "call_id": "call_1",
        "name": "read_file", "arguments": "{\"path\":\"keepalive\"}",
        "status": "completed"
    });
    let mut pending_call = call.clone();
    pending_call["arguments"] = json!("");
    pending_call["status"] = json!("in_progress");
    let expected = vec![
        json!({
            "type": "response.created", "sequence_number": 0,
            "response": {
                "id": "resp_1", "object": "response", "created_at": 0,
                "model": "test-model", "status": "in_progress", "output": []
            }
        }),
        json!({
            "type": "response.output_text.delta", "sequence_number": 1,
            "item_id": "msg_1", "output_index": 0, "content_index": 0,
            "delta": "keepalive", "logprobs": []
        }),
        json!({
            "type": "response.output_item.added", "sequence_number": 2,
            "output_index": 1,
            "item": pending_call
        }),
        json!({
            "type": "response.function_call_arguments.delta", "sequence_number": 3,
            "item_id": "fc_1", "output_index": 1, "delta": "{\"path\":"
        }),
        json!({
            "type": "response.function_call_arguments.delta", "sequence_number": 4,
            "item_id": "fc_1", "output_index": 1, "delta": "\"keepalive\"}"
        }),
        json!({
            "type": "response.function_call_arguments.done", "sequence_number": 5,
            "item_id": "fc_1", "output_index": 1, "name": "read_file",
            "arguments": call["arguments"]
        }),
        json!({
            "type": "response.output_item.done", "sequence_number": 6,
            "output_index": 1, "item": call
        }),
        json!({
            "type": "response.completed", "sequence_number": 7,
            "response": {
                "id": "resp_1", "object": "response", "created_at": 0,
                "model": "test-model", "status": "completed", "output": [call],
                "usage": {
                    "input_tokens": 10, "output_tokens": 5, "total_tokens": 15,
                    "input_tokens_details": { "cached_tokens": 0 },
                    "output_tokens_details": { "reasoning_tokens": 0 }
                }
            }
        }),
    ];
    let mut events = Vec::new();
    for event in &expected {
        events.push(SseEvent::data(r#"{"type":"keepalive","timestamp":123}"#));
        events.push(SseEvent::with_event("keepalive", "ping"));
        events.push(SseEvent::data(event.to_string()));
    }
    events.push(SseEvent::data(r#"{"type":"keepalive"}"#));
    events.push(SseEvent::data("[DONE]"));
    events.push(SseEvent::data(r#"{"type":"unexpected_after_done"}"#));
    server.enqueue_response("/v1/responses", ScriptedResponse::sse(events));

    let actual = response_stream(&server)
        .await
        .map(|event| {
            serde_json::to_value(event.expect("heartbeat must not fail the stream")).unwrap()
        })
        .collect::<Vec<_>>()
        .await;
    let expected = expected
        .iter()
        .map(|event| {
            serde_json::to_value(deserialize_response_event(&event.to_string()).unwrap()).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn keepalive_does_not_hide_unknown_events_or_malformed_json() {
    for data in [r#"{"type":"new_event"}"#, r#"{"type":"ping"}"#, "not json"] {
        let server = MockInferenceServer::start().await.unwrap();
        server.enqueue_response(
            "/v1/responses",
            ScriptedResponse::sse(vec![
                SseEvent::data(r#"{"type":"keepalive"}"#),
                SseEvent::data(data),
                SseEvent::data("[DONE]"),
            ]),
        );
        let events = response_stream(&server).await.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 1, "{data}: {events:?}");
        assert!(matches!(events[0], Err(SamplingError::Serialization(_))));
    }
}

#[tokio::test]
async fn keepalive_does_not_hide_server_errors() {
    let server = MockInferenceServer::start().await.unwrap();
    server.enqueue_response(
        "/v1/responses",
        ScriptedResponse::sse(vec![
            SseEvent::data(r#"{"type":"keepalive"}"#),
            SseEvent::data(r#"{"error":{"type":"server_error","message":"upstream failed"}}"#),
            SseEvent::data("[DONE]"),
        ]),
    );
    let events = response_stream(&server).await.collect::<Vec<_>>().await;
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        Err(SamplingError::StreamError { message, .. }) if message == "upstream failed"
    ));
}

#[tokio::test]
async fn keepalive_and_sse_comments_can_end_without_content() {
    let server = MockInferenceServer::start().await.unwrap();
    let mut response = ScriptedResponse::text(
        200,
        ": keepalive\n\nevent: keepalive\ndata: ping\n\ndata: {\"type\":\"keepalive\"}\n\n",
    );
    response
        .headers
        .push(("content-type".into(), "text/event-stream".into()));
    server.enqueue_response("/v1/responses", response);
    assert!(response_stream(&server).await.next().await.is_none());
}

#[tokio::test]
async fn keepalive_does_not_reset_idle_timeout() {
    let server = MockInferenceServer::start().await.unwrap();
    server.set_chunk_delay(Some(Duration::from_millis(10)));
    server.enqueue_response(
        "/v1/responses",
        ScriptedResponse::sse(vec![SseEvent::data(r#"{"type":"keepalive"}"#); 1_000]),
    );
    let raw = response_stream(&server).await;
    let events = tokio::time::timeout(
        Duration::from_secs(2),
        stream_responses(
            raw,
            None,
            RequestId::random(),
            Duration::from_millis(100),
            None,
        )
        .collect::<Vec<_>>(),
    )
    .await
    .expect("heartbeat-only stream must time out");
    assert!(
        events.iter().any(|event| matches!(
            event,
            SamplingEvent::Failed { error, .. } if error.kind == SamplingErrorKind::IdleTimeout
        )),
        "{events:?}"
    );
}
