use super::reasoning_portability::reasoning_is_portable_to_gemini;
use super::*;
use crate::gemini::{
    GeminiContent, GeminiFunctionCall, GeminiFunctionDeclaration, GeminiFunctionResponse,
    GeminiFunctionResponsePart, GeminiInlineData, GeminiPart, GeminiTool, GenerateContentRequest,
    GenerationConfig, ThinkingConfig, json_schema_to_gemini_schema, wire_thought_signature,
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
            .filter_map(|part| match part {
                ContentPart::Text { text } if text.is_empty() => None,
                ContentPart::Text { text } => Some(GeminiPart::text(text.as_ref())),
                ContentPart::Image { url } => Some(image_url_to_gemini_part(url)),
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
                let image_parts: Vec<GeminiFunctionResponsePart> = tr
                    .images
                    .iter()
                    .filter_map(image_content_to_function_response_part)
                    .collect();
                pending_fn_responses.push(GeminiPart {
                    function_response: Some(GeminiFunctionResponse {
                        id: Some(tr.tool_call_id.clone()),
                        name,
                        response,
                        parts: (!image_parts.is_empty()).then_some(image_parts),
                    }),
                    ..GeminiPart::default()
                });
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
    sanitize_gemini_contents(&mut contents);

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
                            Some(json_schema_to_gemini_schema(&t.parameters))
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
        response_schema: req.json_schema.as_ref().map(json_schema_to_gemini_schema),
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

fn parse_data_url(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("data:")?;
    let (header, data) = rest
        .split_once(";base64,")
        .or_else(|| rest.split_once(','))?;
    Some((header.to_string(), data.to_string()))
}

fn image_url_to_gemini_part(url: &str) -> GeminiPart {
    if let Some((mime_type, data)) = parse_data_url(url) {
        GeminiPart {
            inline_data: Some(GeminiInlineData { mime_type, data }),
            ..GeminiPart::default()
        }
    } else {
        GeminiPart::text(format!("[image: {url}]"))
    }
}

fn image_content_to_function_response_part(
    part: &ContentPart,
) -> Option<GeminiFunctionResponsePart> {
    let ContentPart::Image { url } = part else {
        return None;
    };
    let (mime_type, data) = parse_data_url(url)?;
    Some(GeminiFunctionResponsePart {
        inline_data: Some(GeminiInlineData { mime_type, data }),
    })
}

fn part_is_empty(part: &GeminiPart) -> bool {
    part.text.as_ref().is_none_or(|t| t.is_empty())
        && part.function_call.is_none()
        && part.function_response.is_none()
        && part.inline_data.is_none()
        && !part.thought
        && part.thought_signature.as_ref().is_none_or(|s| s.is_empty())
}

/// Gemini generateContent requires alternating user/model turns and a user tip.
fn sanitize_gemini_contents(contents: &mut Vec<GeminiContent>) {
    for content in contents.iter_mut() {
        content.parts.retain(|p| !part_is_empty(p));
    }
    contents.retain(|c| !c.parts.is_empty());
    let mut merged: Vec<GeminiContent> = Vec::new();
    for content in contents.drain(..) {
        if let Some(last) = merged.last_mut()
            && last.role == content.role
        {
            last.parts.extend(content.parts);
        } else {
            merged.push(content);
        }
    }
    *contents = merged;
    while contents.last().and_then(|c| c.role.as_deref()) == Some("model") {
        contents.pop();
    }
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
        let params = g
            .tools
            .as_ref()
            .and_then(|tools| tools.first())
            .and_then(|tool| tool.function_declarations.as_ref())
            .and_then(|decls| decls.first())
            .and_then(|decl| decl.parameters.as_ref());
        assert_eq!(
            params.and_then(|p| p.get("type")).and_then(|t| t.as_str()),
            Some("OBJECT")
        );
        assert!(g.tools.is_some());
    }

    #[test]
    fn function_parameters_are_gemini_schema_not_json_schema() {
        let req = ConversationRequest {
            items: vec![user("hi")],
            tools: vec![ToolSpec {
                name: "todo_write".into(),
                description: Some("todos".into()),
                parameters: serde_json::json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["todos"],
                    "properties": {
                        "todos": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "content": { "type": ["string", "null"] },
                                    "status": {
                                        "type": ["string", "null"],
                                        "enum": ["pending", null]
                                    }
                                }
                            }
                        }
                    }
                }),
            }],
            ..Default::default()
        };
        let g = build_gemini_request(&req);
        let wire = serde_json::to_value(&g).expect("serialize");
        let wire_text = wire.to_string();
        assert!(
            !wire_text.contains("$schema"),
            "Gemini proto Schema rejects $schema: {wire_text}"
        );
        assert!(
            !wire_text.contains("additionalProperties"),
            "Gemini proto Schema rejects additionalProperties: {wire_text}"
        );
        assert!(
            !wire_text.contains(r#""type":["string""#),
            "Gemini proto Schema type must be an uppercase enum, not a JSON Schema union: {wire_text}"
        );
        let params = wire
            .pointer("/tools/0/functionDeclarations/0/parameters")
            .expect("parameters");
        assert_eq!(params.get("type").and_then(|t| t.as_str()), Some("OBJECT"));
        let content_type = params.pointer("/properties/todos/items/properties/content/type");
        assert_eq!(content_type.and_then(|t| t.as_str()), Some("STRING"));
        assert_eq!(
            params.pointer("/properties/todos/items/properties/content/nullable"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn tool_result_images_live_inside_function_response_parts() {
        let req = ConversationRequest {
            items: vec![
                user("look"),
                ConversationItem::Reasoning(rs::ReasoningItem {
                    id: String::new(),
                    summary: vec![rs::SummaryPart::SummaryText(rs::SummaryTextContent {
                        text: "reading screenshot".into(),
                    })],
                    content: None,
                    encrypted_content: Some(store_thought_signature("SIG")),
                    status: None,
                }),
                ConversationItem::assistant_tool_calls(vec![ToolCall {
                    id: Arc::from("call_img"),
                    name: "read_file".into(),
                    arguments: Arc::from(r#"{"path":"shot.jpg"}"#),
                }]),
                ConversationItem::tool_result_with_images(
                    "call_img",
                    "Read image file: shot.jpg",
                    vec![ContentPart::Image {
                        url: Arc::from("data:image/jpeg;base64,/9j/4AAQ"),
                    }],
                ),
            ],
            ..Default::default()
        };
        let g = build_gemini_request(&req);
        let last = g.contents.last().expect("contents");
        assert_eq!(last.role.as_deref(), Some("user"));
        assert!(
            last.parts.iter().all(|p| p.inline_data.is_none()),
            "sibling inlineData next to functionResponse 400s Gemini 3: {last:?}"
        );
        let fr = last
            .parts
            .iter()
            .find_map(|p| p.function_response.as_ref())
            .expect("functionResponse");
        let media = fr.parts.as_ref().expect("functionResponse.parts");
        assert_eq!(media.len(), 1);
        assert_eq!(
            media
                .first()
                .and_then(|p| p.inline_data.as_ref())
                .map(|d| d.mime_type.as_str()),
            Some("image/jpeg")
        );
        assert_eq!(
            g.contents.last().and_then(|c| c.role.as_deref()),
            Some("user"),
            "generateContent cannot end on a model turn"
        );
    }

    #[test]
    fn user_prompt_images_stay_content_parts_and_end_on_user() {
        let mut u = ConversationItem::user("see this");
        u.add_image("data:image/png;base64,iVBORw0K");
        let req = ConversationRequest {
            items: vec![
                u,
                ConversationItem::assistant("ok"),
                ConversationItem::user("again"),
            ],
            ..Default::default()
        };
        let g = build_gemini_request(&req);
        assert_eq!(
            g.contents.first().and_then(|c| c.role.as_deref()),
            Some("user")
        );
        assert!(
            g.contents
                .first()
                .map(|c| c.parts.iter().any(|p| p.inline_data.is_some()))
                .unwrap_or(false)
        );
        assert_eq!(
            g.contents.last().and_then(|c| c.role.as_deref()),
            Some("user")
        );
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
