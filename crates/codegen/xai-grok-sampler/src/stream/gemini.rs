//! Layer-2 stream transform for Google Gemini `streamGenerateContent` SSE.

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::stream::{BoxStream, Stream};

use xai_grok_sampling_types::gemini::{GenerateContentResponse, store_thought_signature};
use xai_grok_sampling_types::{
    AssistantItem, ConversationItem, ConversationResponse, ResponseModelMetadata, SamplingError,
    StopReason, TokenUsage, ToolCall, rs,
};

use crate::events::{SamplingChannel, SamplingErrorInfo, SamplingEvent};
use crate::metrics::InferenceLatencyStats;
use crate::types::RequestId;

pub fn stream_gemini<'a>(
    raw_stream: BoxStream<'a, Result<GenerateContentResponse, SamplingError>>,
    model_metadata: Option<ResponseModelMetadata>,
    request_id: RequestId,
    idle_timeout: Duration,
) -> impl Stream<Item = SamplingEvent> + Send + 'a {
    async_stream::stream! {
        let decode_region = crate::span_timing::Region::from_span(tracing::info_span!(
            "sampling.stream_decode",
            ttft_ms = tracing::field::Empty,
            ttlb_ms = tracing::field::Empty,
            output_tokens = tracing::field::Empty,
            chunk_count = tracing::field::Empty,
        ));
        let stream_start = Instant::now();
        let mut chunk_timestamps: Vec<Instant> = Vec::new();

        yield SamplingEvent::StreamStarted {
            request_id: request_id.clone(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };

        if let Some(metadata) = model_metadata {
            yield SamplingEvent::ModelMetadata {
                request_id: request_id.clone(),
                metadata,
            };
        }

        let mut assistant_text = String::new();
        let mut thinking_acc = String::new();
        let mut thought_signature: Option<String> = None;
        let mut assistant_tool_calls: Vec<ToolCall> = Vec::new();
        let mut final_model: Option<String> = None;
        let mut final_stop_reason: Option<StopReason> = None;
        let mut final_stop_message: Option<String> = None;
        let mut final_message_id: Option<String> = None;
        let mut final_raw_stop_reason: Option<String> = None;
        let mut usage = TokenUsage::default();
        let mut chunk_index: u64 = 0;
        let mut message_chunk_count: u64 = 0;
        let mut first_token_emitted = false;
        let mut stream = raw_stream;

        loop {
            let event_result = match tokio::time::timeout(idle_timeout, stream.next()).await {
                Ok(Some(event_result)) => event_result,
                Ok(None) => break,
                Err(_elapsed) => {
                    let err = SamplingError::IdleTimeout {
                        elapsed_secs: idle_timeout.as_secs(),
                    };
                    yield SamplingEvent::Failed {
                        request_id: request_id.clone(),
                        error: SamplingErrorInfo::from(&err),
                    };
                    return;
                }
            };

            let chunk = match event_result {
                Ok(chunk) => chunk,
                Err(err) => {
                    yield SamplingEvent::Failed {
                        request_id: request_id.clone(),
                        error: SamplingErrorInfo::from(&err),
                    };
                    return;
                }
            };

            if let Some(id) = chunk.response_id.clone() {
                final_message_id = Some(id);
            }
            if let Some(model) = chunk.model_version.clone() {
                final_model = Some(model);
            }
            if let Some(feedback) = &chunk.prompt_feedback
                && let Some(reason) = feedback.block_reason.as_deref()
                && !reason.is_empty()
            {
                final_stop_reason = Some(StopReason::ContentFilter);
                final_raw_stop_reason = Some(reason.to_string());
                final_stop_message = feedback.block_reason_message.clone();
            }
            if let Some(meta) = &chunk.usage_metadata {
                if let Some(p) = meta.prompt_token_count {
                    usage.prompt_tokens = p;
                }
                if let Some(c) = meta.candidates_token_count {
                    usage.completion_tokens = c;
                }
                if let Some(t) = meta.total_token_count {
                    usage.total_tokens = t;
                }
                if let Some(r) = meta.thoughts_token_count {
                    usage.reasoning_tokens = r;
                }
                if let Some(cached) = meta.cached_content_token_count {
                    usage.cached_prompt_tokens = cached;
                }
            }

            let candidate = chunk.candidates.first();
            if let Some(c) = candidate {
                if let Some(reason) = c.finish_reason.as_deref().filter(|r| !r.is_empty()) {
                    final_raw_stop_reason = Some(reason.to_string());
                    final_stop_reason = Some(map_finish_reason(reason));
                }
                if let Some(content) = &c.content {
                    for part in &content.parts {
                        if let Some(sig) = part.thought_signature.as_deref().filter(|s| !s.is_empty())
                        {
                            thought_signature = Some(store_thought_signature(sig));
                        }
                        if let Some(text) = part.text.as_deref().filter(|t| !t.is_empty()) {
                            if !first_token_emitted {
                                first_token_emitted = true;
                                yield SamplingEvent::FirstToken {
                                    request_id: request_id.clone(),
                                };
                            }
                            chunk_index += 1;
                            let channel = if part.thought {
                                thinking_acc.push_str(text);
                                SamplingChannel::Reasoning
                            } else {
                                assistant_text.push_str(text);
                                message_chunk_count += 1;
                                chunk_timestamps.push(Instant::now());
                                SamplingChannel::Text
                            };
                            yield SamplingEvent::ChannelToken {
                                request_id: request_id.clone(),
                                channel,
                                text: text.to_string(),
                                chunk_index,
                            };
                        }
                        if let Some(fc) = &part.function_call {
                            if !first_token_emitted {
                                first_token_emitted = true;
                                yield SamplingEvent::FirstToken {
                                    request_id: request_id.clone(),
                                };
                            }
                            let tool_index = assistant_tool_calls.len() as u32;
                            let id = fc
                                .id
                                .clone()
                                .unwrap_or_else(|| format!("gemini-fn-{tool_index}"));
                            let args = fc
                                .args
                                .as_ref()
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "{}".to_string());
                            yield SamplingEvent::ToolCallDelta {
                                request_id: request_id.clone(),
                                tool_index,
                                id: Some(id.clone()),
                                name: Some(fc.name.clone()),
                                arguments_delta: Some(args.clone()),
                            };
                            assistant_tool_calls.push(ToolCall {
                                id: std::sync::Arc::<str>::from(id),
                                name: fc.name.clone(),
                                arguments: std::sync::Arc::<str>::from(args),
                            });
                        }
                    }
                }
            }
        }

        if thought_signature.is_some() {
            yield SamplingEvent::ReasoningCompleted {
                request_id: request_id.clone(),
                signature: thought_signature.clone().unwrap_or_default(),
            };
        }

        let stop_reason = final_stop_reason.or({
            if assistant_tool_calls.is_empty() {
                Some(StopReason::Stop)
            } else {
                Some(StopReason::ToolCalls)
            }
        });
        if matches!(stop_reason, Some(StopReason::ToolCalls)) && final_raw_stop_reason.is_none() {
            final_raw_stop_reason = Some("STOP".into());
        }

        let assistant_item = ConversationItem::Assistant(AssistantItem {
            content: std::sync::Arc::<str>::from(assistant_text),
            tool_calls: assistant_tool_calls,
            model_id: final_model,
            model_fingerprint: None,
            reasoning_effort: None,
        });

        let mut items: Vec<ConversationItem> = Vec::new();
        if !thinking_acc.is_empty() || thought_signature.is_some() {
            let summary = if thinking_acc.is_empty() {
                vec![]
            } else {
                vec![rs::SummaryPart::SummaryText(rs::SummaryTextContent {
                    text: thinking_acc,
                })]
            };
            items.push(ConversationItem::Reasoning(rs::ReasoningItem {
                id: String::new(),
                summary,
                content: None,
                encrypted_content: thought_signature,
                status: None,
            }));
        }
        items.push(assistant_item);

        let stream_end = Instant::now();
        let metrics =
            InferenceLatencyStats::from_timestamps(stream_start, &chunk_timestamps, stream_end);
        decode_region
            .span()
            .record("ttlb_ms", metrics.time_to_last_byte_ms as i64);
        decode_region
            .span()
            .record("chunk_count", metrics.chunk_count as i64);
        if let Some(ttft) = metrics.time_to_first_token_ms {
            decode_region.span().record("ttft_ms", ttft as i64);
        }
        decode_region
            .span()
            .record("output_tokens", usage.completion_tokens as i64);
        drop(decode_region);

        let usage = if usage.prompt_tokens == 0
            && usage.completion_tokens == 0
            && usage.total_tokens == 0
        {
            None
        } else {
            Some(usage)
        };

        yield SamplingEvent::Completed {
            request_id: request_id.clone(),
            response: Box::new(ConversationResponse {
                items,
                stop_reason,
                usage,
                cost_usd_ticks: None,
                message_chunks_emitted: message_chunk_count,
                doom_loop_signals: Vec::new(),
                stop_message: final_stop_message,
                message_id: final_message_id,
                raw_stop_reason: final_raw_stop_reason,
                stop_sequence: None,
            }),
            metrics,
        };
    }
}

fn map_finish_reason(reason: &str) -> StopReason {
    match reason {
        "STOP" => StopReason::Stop,
        "MAX_TOKENS" => StopReason::Length,
        "SAFETY" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY" | "RECITATION" => {
            StopReason::ContentFilter
        }
        other if other.to_ascii_uppercase().contains("TOOL") => StopReason::ToolCalls,
        _ => StopReason::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn rid() -> RequestId {
        RequestId::from("req-1")
    }

    #[tokio::test]
    async fn streams_text_thought_and_function_call() {
        let chunk = serde_json::from_value::<GenerateContentResponse>(serde_json::json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"text": "plan", "thought": true},
                        {"thoughtSignature": "SIG"},
                        {"text": "hello"},
                        {"functionCall": {"name": "read_file", "args": {"path": "a.rs"}}}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 4,
                "totalTokenCount": 14,
                "thoughtsTokenCount": 2
            },
            "responseId": "resp-1",
            "modelVersion": "gemini-3.8-flash"
        }))
        .unwrap();
        let raw = stream::iter(vec![Ok(chunk)]).boxed();
        let events: Vec<_> = stream_gemini(raw, None, rid(), Duration::from_secs(5))
            .collect()
            .await;
        assert!(matches!(
            events.first(),
            Some(SamplingEvent::StreamStarted { .. })
        ));
        assert!(events.iter().any(|e| matches!(
            e,
            SamplingEvent::ChannelToken { channel: SamplingChannel::Reasoning, text, .. } if text == "plan"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            SamplingEvent::ChannelToken { channel: SamplingChannel::Text, text, .. } if text == "hello"
        )));
        let SamplingEvent::Completed { response, .. } = events.last().unwrap() else {
            panic!("expected completed");
        };
        assert_eq!(response.assistant_text(), "hello");
        assert_eq!(
            response
                .assistant()
                .and_then(|a| a.tool_calls.first())
                .map(|t| t.name.as_str()),
            Some("read_file")
        );
        let reasoning = response.reasoning_items().next().unwrap();
        assert_eq!(reasoning.encrypted_content.as_deref(), Some("gsig:SIG"));
        assert_eq!(response.stop_reason, Some(StopReason::Stop));
        assert_eq!(response.usage.as_ref().unwrap().reasoning_tokens, 2);
    }
}
