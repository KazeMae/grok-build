use super::reasoning_portability::reasoning_is_portable_to_gemini;
use super::*;
use crate::gemini::{
    GeminiContent, GeminiFunctionCall, GeminiFunctionDeclaration, GeminiFunctionResponse,
    GeminiInlineData, GeminiPart, GeminiTool, GenerateContentRequest, GenerationConfig,
    ThinkingConfig, wire_thought_signature,
};

/// Map a conversation onto Gemini `generateContent` JSON (python-genai REST).
pub fn build_gemini_request(req: &ConversationRequest) -> GenerateContentRequest {
    let mut system_parts: Vec<GeminiPart> = Vec::new();
    let mut contents: Vec<GeminiContent> = Vec::new();
    let mut pending_model: Vec<GeminiPart> = Vec::new();
    let mut pending_fn_responses: Vec<GeminiPart> = Vec::new();
    let mut tool_id_to_name: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut pending_signature: Option<String> = None;

    let flush_model = |pending: &mut Vec<GeminiPart>, contents: &mut Vec<GeminiContent>| {
        if !pending.is_empty() {
            contents.push(GeminiContent {
                role: Some("model".into()),
                parts: std::mem::take(pending),
            });
        }
    };
    let flush_fn_responses = |pending: &mut Vec<GeminiPart>, contents: &mut Vec<GeminiContent>| {
        if !pending.is_empty() {
            contents.push(GeminiContent {
                role: Some("user".into()),
                parts: std::mem::take(pending),
            });
        }
    };

    let content_parts_to_gemini = |parts: &[ContentPart]| -> Vec<GeminiPart> {
        parts
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => GeminiPart::text(text.as_ref()),
                ContentPart::Image { url } => {
                    if let Some((header, data)) = url.strip_prefix("data:").and_then(|rest| {
                        rest.split_once(";base64,").or_else(|| rest.split_once(','))
                    }) {
                        GeminiPart {
                            inline_data: Some(GeminiInlineData {
                                mime_type: header.to_string(),
                                data: data.to_string(),
                            }),
                            ..GeminiPart::default()
                        }
                    } else {
                        GeminiPart::text(format!("[image: {url}]"))
                    }
                }
            })
            .collect()
    };

    for item in &req.items {
        match item {
            ConversationItem::System(s) => {
                flush_model(&mut pending_model, &mut contents);
                flush_fn_responses(&mut pending_fn_responses, &mut contents);
                system_parts.push(GeminiPart::text(s.content.as_ref()));
            }
            ConversationItem::User(u) => {
                flush_model(&mut pending_model, &mut contents);
                flush_fn_responses(&mut pending_fn_responses, &mut contents);
                contents.push(GeminiContent {
                    role: Some("user".into()),
                    parts: content_parts_to_gemini(&u.content),
                });
            }
            ConversationItem::Assistant(a) => {
                flush_fn_responses(&mut pending_fn_responses, &mut contents);
                if !a.content.is_empty() {
                    pending_model.push(GeminiPart::text(a.content.as_ref()));
                }
                for (i, tc) in a.tool_calls.iter().enumerate() {
                    tool_id_to_name.insert(tc.id.to_string(), tc.name.clone());
                    let args = serde_json::from_str(&tc.arguments).ok();
                    let mut part = GeminiPart {
                        function_call: Some(GeminiFunctionCall {
                            id: Some(tc.id.to_string()),
                            name: tc.name.clone(),
                            args,
                        }),
                        ..GeminiPart::default()
                    };
                    if i == 0
                        && let Some(sig) = pending_signature.take()
                    {
                        part.thought_signature = Some(sig);
                    }
                    pending_model.push(part);
                }
            }
            ConversationItem::ToolResult(tr) => {
                flush_model(&mut pending_model, &mut contents);
                let name = tool_id_to_name
                    .get(&tr.tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| tr.tool_call_id.clone());
                let response = serde_json::from_str::<serde_json::Value>(&tr.content)
                    .unwrap_or_else(|_| serde_json::json!({ "output": tr.content.as_ref() }));
                pending_fn_responses.push(GeminiPart {
                    function_response: Some(GeminiFunctionResponse {
                        id: Some(tr.tool_call_id.clone()),
                        name,
                        response,
                    }),
                    ..GeminiPart::default()
                });
                for image in &tr.images {
                    pending_fn_responses
                        .extend(content_parts_to_gemini(std::slice::from_ref(image)));
                }
            }
            ConversationItem::BackendToolCall(b) => {
                flush_fn_responses(&mut pending_fn_responses, &mut contents);
                pending_model.push(GeminiPart::text(b.text_summary()));
            }
            ConversationItem::Reasoning(r) => {
                if !reasoning_is_portable_to_gemini(r) {
                    continue;
                }
                flush_fn_responses(&mut pending_fn_responses, &mut contents);
                let thinking = reasoning_item_text(r);
                let signature = r
                    .encrypted_content
                    .as_deref()
                    .and_then(wire_thought_signature)
                    .map(str::to_owned);
                if let Some(sig) = signature.clone() {
                    pending_signature = Some(sig);
                }
                if !thinking.is_empty() {
                    pending_model.push(GeminiPart::thought_text(thinking, signature));
                }
            }
        }
    }

    flush_model(&mut pending_model, &mut contents);
    flush_fn_responses(&mut pending_fn_responses, &mut contents);

    let tools = if req.tools.is_empty() {
        None
    } else {
        Some(vec![GeminiTool {
            function_declarations: Some(
                req.tools
                    .iter()
                    .map(|t| GeminiFunctionDeclaration {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: if t.parameters.is_null() {
                            None
                        } else {
                            Some(t.parameters.clone())
                        },
                    })
                    .collect(),
            ),
        }])
    };

    let thinking_budget = req.reasoning_effort.map(|effort| match effort {
        crate::ReasoningEffort::None | crate::ReasoningEffort::Minimal => 0,
        crate::ReasoningEffort::Low => 1024,
        crate::ReasoningEffort::Medium => 8192,
        crate::ReasoningEffort::High => 16384,
        crate::ReasoningEffort::Xhigh | crate::ReasoningEffort::Max => -1,
    });

    let generation_config = Some(GenerationConfig {
        temperature: req.temperature,
        top_p: req.top_p,
        max_output_tokens: req.max_output_tokens,
        thinking_config: Some(ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget,
        }),
        response_mime_type: req
            .json_schema
            .as_ref()
            .map(|_| "application/json".to_string()),
        response_schema: req.json_schema.clone(),
    });

    GenerateContentRequest {
        model: req.model.clone(),
        contents,
        system_instruction: if system_parts.is_empty() {
            None
        } else {
            Some(GeminiContent {
                role: None,
                parts: system_parts,
            })
        },
        tools,
        generation_config,
    }
}

/// Strip a trailing `/v1` or `/v1beta` so Gemini URLs land on `{root}/v1beta/models/...`.
pub fn gemini_api_root(base_url: &str) -> String {
    let mut root = base_url.trim().trim_end_matches('/').to_string();
    if let Some((plain, _)) = root.split_once('?') {
        root = plain.trim_end_matches('/').to_string();
    }
    for suffix in ["/v1beta", "/v1"] {
        if let Some(stripped) = root.strip_suffix(suffix) {
            root = stripped.trim_end_matches('/').to_string();
            break;
        }
    }
    root
}

pub fn gemini_generate_path(model: &str, stream: bool) -> String {
    let model = model.trim().trim_start_matches("models/");
    let method = if stream {
        "streamGenerateContent"
    } else {
        "generateContent"
    };
    format!("v1beta/models/{model}:{method}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini::{GEMINI_THOUGHT_SIG_PREFIX, store_thought_signature};

    fn user(text: &str) -> ConversationItem {
        ConversationItem::user(text)
    }

    #[test]
    fn maps_user_and_system() {
        let req = ConversationRequest {
            items: vec![ConversationItem::system("be concise"), user("hi")],
            model: Some("gemini-3.8-flash".into()),
            ..Default::default()
        };
        let g = build_gemini_request(&req);
        assert_eq!(g.model.as_deref(), Some("gemini-3.8-flash"));
        assert_eq!(
            g.system_instruction
                .as_ref()
                .and_then(|c| c.parts.first())
                .and_then(|p| p.text.as_deref()),
            Some("be concise")
        );
        assert_eq!(g.contents.len(), 1);
        assert_eq!(
            g.contents.first().and_then(|c| c.role.as_deref()),
            Some("user")
        );
        assert_eq!(
            g.generation_config
                .as_ref()
                .and_then(|c| c.thinking_config.as_ref())
                .and_then(|t| t.include_thoughts),
            Some(true)
        );
    }

    #[test]
    fn maps_tool_round_trip_and_thought_signature() {
        let req = ConversationRequest {
            items: vec![
                user("read it"),
                ConversationItem::Reasoning(rs::ReasoningItem {
                    id: String::new(),
                    summary: vec![rs::SummaryPart::SummaryText(rs::SummaryTextContent {
                        text: "thinking".into(),
                    })],
                    content: None,
                    encrypted_content: Some(store_thought_signature("SIGBLOB")),
                    status: None,
                }),
                ConversationItem::assistant_tool_calls(vec![ToolCall {
                    id: Arc::from("call-1"),
                    name: "read_file".into(),
                    arguments: Arc::from(r#"{"path":"a.rs"}"#),
                }]),
                ConversationItem::tool_result("call-1", "ok"),
            ],
            tools: vec![ToolSpec {
                name: "read_file".into(),
                description: Some("read".into()),
                parameters: serde_json::json!({"type":"object"}),
            }],
            ..Default::default()
        };
        let g = build_gemini_request(&req);
        assert_eq!(g.contents.len(), 3);
        let model_turn = g.contents.get(1).expect("model turn");
        assert_eq!(model_turn.role.as_deref(), Some("model"));
        let model_parts = &model_turn.parts;
        assert!(
            model_parts
                .iter()
                .any(|p| p.thought && p.text.as_deref() == Some("thinking"))
        );
        let fc = model_parts
            .iter()
            .find_map(|p| p.function_call.as_ref())
            .expect("functionCall");
        assert_eq!(fc.name, "read_file");
        assert_eq!(
            model_parts
                .iter()
                .find_map(|p| p.thought_signature.as_deref()),
            Some("SIGBLOB")
        );
        let user_turn = g.contents.get(2).expect("tool result turn");
        assert_eq!(user_turn.role.as_deref(), Some("user"));
        assert_eq!(
            user_turn
                .parts
                .first()
                .and_then(|p| p.function_response.as_ref())
                .map(|r| r.name.as_str()),
            Some("read_file")
        );
        assert!(g.tools.is_some());
    }

    #[test]
    fn drops_foreign_reasoning() {
        let req = ConversationRequest {
            items: vec![
                user("hi"),
                ConversationItem::Reasoning(rs::ReasoningItem {
                    id: "rs_abc".into(),
                    summary: vec![],
                    content: None,
                    encrypted_content: Some("gAAAAAencrypted".into()),
                    status: None,
                }),
            ],
            ..Default::default()
        };
        let g = build_gemini_request(&req);
        assert!(
            g.contents
                .iter()
                .flat_map(|c| &c.parts)
                .all(|p| !p.thought && p.thought_signature.is_none())
        );
    }

    #[test]
    fn api_root_strips_openai_v1() {
        assert_eq!(
            gemini_api_root("http://alb.example/v1"),
            "http://alb.example"
        );
        assert_eq!(
            gemini_api_root("https://generativelanguage.googleapis.com"),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            gemini_api_root("https://generativelanguage.googleapis.com/v1beta/"),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            gemini_generate_path("gemini-3.8-flash", true),
            "v1beta/models/gemini-3.8-flash:streamGenerateContent"
        );
        assert!(GEMINI_THOUGHT_SIG_PREFIX.starts_with("gsig"));
    }
}
