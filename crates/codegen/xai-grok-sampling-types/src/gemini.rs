//! Google Gemini `generateContent` / `streamGenerateContent` wire types.
//!
//! Matches the REST JSON used by
//! [`googleapis/python-genai`](https://github.com/googleapis/python-genai):
//! `POST {root}/v1beta/models/{model}:streamGenerateContent?alt=sse`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Stored on [`crate::rs::ReasoningItem::encrypted_content`] so other backends
/// can drop Gemini thought signatures without guessing at opaque blobs.
pub const GEMINI_THOUGHT_SIG_PREFIX: &str = "gsig:";

pub fn store_thought_signature(sig: &str) -> String {
    if sig.starts_with(GEMINI_THOUGHT_SIG_PREFIX) {
        sig.to_string()
    } else {
        format!("{GEMINI_THOUGHT_SIG_PREFIX}{sig}")
    }
}

pub fn wire_thought_signature(stored: &str) -> Option<&str> {
    if let Some(sig) = stored.strip_prefix(GEMINI_THOUGHT_SIG_PREFIX) {
        Some(sig)
    } else if stored.is_empty() {
        None
    } else {
        Some(stored)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub parts: Vec<GeminiPart>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub thought: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<GeminiFunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<GeminiInlineData>,
}

impl GeminiPart {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    pub fn thought_text(text: impl Into<String>, signature: Option<String>) -> Self {
        Self {
            text: Some(text.into()),
            thought: true,
            thought_signature: signature,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub response: serde_json::Value,
    /// Media from the tool (screenshots, `read_file` images). Must live here, not as
    /// sibling `inlineData` parts: Gemini 3 treats extra user parts next to a
    /// `functionResponse` as a new turn and 400s with "ending with a model turn".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<GeminiFunctionResponsePart>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionResponsePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<GeminiInlineData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiInlineData {
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_declarations: Option<Vec<GeminiFunctionDeclaration>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(default)]
    pub candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    pub usage_metadata: Option<GeminiUsageMetadata>,
    #[serde(default)]
    pub prompt_feedback: Option<GeminiPromptFeedback>,
    #[serde(default)]
    pub model_version: Option<String>,
    #[serde(default)]
    pub response_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCandidate {
    #[serde(default)]
    pub content: Option<GeminiContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiUsageMetadata {
    #[serde(default)]
    pub prompt_token_count: Option<u32>,
    #[serde(default)]
    pub candidates_token_count: Option<u32>,
    #[serde(default)]
    pub total_token_count: Option<u32>,
    #[serde(default)]
    pub thoughts_token_count: Option<u32>,
    #[serde(default)]
    pub cached_content_token_count: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPromptFeedback {
    #[serde(default)]
    pub block_reason: Option<String>,
    #[serde(default)]
    pub block_reason_message: Option<String>,
}

impl GenerateContentResponse {
    pub fn has_meaningful_content(&self) -> bool {
        if self
            .prompt_feedback
            .as_ref()
            .and_then(|f| f.block_reason.as_deref())
            .is_some_and(|r| !r.is_empty())
        {
            return true;
        }
        self.candidates.iter().any(|c| {
            c.finish_reason.as_deref().is_some_and(|r| !r.is_empty())
                || c.content.as_ref().is_some_and(|content| {
                    content.parts.iter().any(|p| {
                        p.text.as_deref().is_some_and(|t| !t.is_empty())
                            || p.function_call.is_some()
                            || p.thought_signature
                                .as_deref()
                                .is_some_and(|s| !s.is_empty())
                    })
                })
        })
    }
}

/// Convert JSON Schema into Gemini `Schema` proto JSON.
///
/// `FunctionDeclaration.parameters` is an OpenAPI Schema proto, not JSON Schema.
/// Keywords such as `$schema`, `$ref`, and `additionalProperties`, plus `type`
/// as a lowercase string or union array, are rejected with HTTP 400.
pub fn json_schema_to_gemini_schema(schema: &Value) -> Value {
    if schema.is_null() {
        return json!({"type": "OBJECT"});
    }
    let defs = schema
        .as_object()
        .and_then(|o| o.get("$defs").or_else(|| o.get("definitions")))
        .and_then(Value::as_object);
    let converted = convert_schema(schema, defs, &mut Vec::new());
    match converted {
        Value::Object(mut obj)
            if obj.contains_key("properties")
                && !obj.contains_key("type")
                && !obj.contains_key("anyOf") =>
        {
            obj.insert("type".into(), json!("OBJECT"));
            Value::Object(obj)
        }
        Value::Object(obj) if obj.is_empty() => json!({"type": "OBJECT"}),
        other => other,
    }
}

fn convert_schema(
    schema: &Value,
    defs: Option<&Map<String, Value>>,
    ref_stack: &mut Vec<String>,
) -> Value {
    let Value::Object(map) = schema else {
        return schema.clone();
    };

    if let Some(all_of) = map.get("allOf").and_then(Value::as_array) {
        return merge_all_of(all_of, map, defs, ref_stack);
    }

    if let Some(r) = map.get("$ref").and_then(Value::as_str) {
        return overlay_resolved_ref(r, map, defs, ref_stack);
    }

    let mut out = Map::new();
    apply_type_field(map, &mut out);
    apply_enum_field(map, &mut out);
    apply_const_field(map, &mut out);

    if let Some(props) = map.get("properties").and_then(Value::as_object) {
        let mut converted = Map::new();
        for (k, v) in props {
            converted.insert(k.clone(), convert_schema(v, defs, ref_stack));
        }
        out.insert("properties".into(), Value::Object(converted));
    }

    if let Some(items) = map.get("items") {
        let converted = if let Some(arr) = items.as_array() {
            arr.first()
                .map(|first| convert_schema(first, defs, ref_stack))
                .unwrap_or_else(|| json!({"type": "STRING"}))
        } else {
            convert_schema(items, defs, ref_stack)
        };
        out.insert("items".into(), converted);
    }

    if let Some(any_of) = map.get("anyOf").and_then(Value::as_array) {
        flatten_any_of(
            any_of
                .iter()
                .map(|v| convert_schema(v, defs, ref_stack))
                .collect(),
            &mut out,
        );
    } else if let Some(one_of) = map.get("oneOf").and_then(Value::as_array) {
        flatten_any_of(
            one_of
                .iter()
                .map(|v| convert_schema(v, defs, ref_stack))
                .collect(),
            &mut out,
        );
    }

    copy_passthrough_fields(map, &mut out);
    finish_object(out)
}

fn merge_all_of(
    all_of: &[Value],
    map: &Map<String, Value>,
    defs: Option<&Map<String, Value>>,
    ref_stack: &mut Vec<String>,
) -> Value {
    let mut merged = Map::new();
    for part in all_of {
        merge_schema_objects(&mut merged, convert_schema(part, defs, ref_stack));
    }
    let mut siblings = map.clone();
    siblings.remove("allOf");
    if !siblings.is_empty() {
        merge_schema_objects(
            &mut merged,
            convert_schema(&Value::Object(siblings), defs, ref_stack),
        );
    }
    finish_object(merged)
}

fn overlay_resolved_ref(
    r: &str,
    map: &Map<String, Value>,
    defs: Option<&Map<String, Value>>,
    ref_stack: &mut Vec<String>,
) -> Value {
    let resolved = resolve_ref(r, defs, ref_stack).unwrap_or_else(|| json!({}));
    let mut overlay = map.clone();
    overlay.remove("$ref");
    if overlay.is_empty() {
        return resolved;
    }
    let mut merged = match resolved {
        Value::Object(obj) => obj,
        other => {
            let mut obj = Map::new();
            obj.insert("const".into(), other);
            obj
        }
    };
    merge_schema_objects(
        &mut merged,
        convert_schema(&Value::Object(overlay), defs, ref_stack),
    );
    finish_object(merged)
}

fn resolve_ref(
    r: &str,
    defs: Option<&Map<String, Value>>,
    ref_stack: &mut Vec<String>,
) -> Option<Value> {
    let name = r
        .strip_prefix("#/$defs/")
        .or_else(|| r.strip_prefix("#/definitions/"))?;
    if ref_stack.iter().any(|s| s == name) {
        return None;
    }
    let def = defs?.get(name)?;
    ref_stack.push(name.to_string());
    let converted = convert_schema(def, defs, ref_stack);
    ref_stack.pop();
    Some(converted)
}

fn apply_type_field(map: &Map<String, Value>, out: &mut Map<String, Value>) {
    let Some(t) = map.get("type") else {
        return;
    };
    match t {
        Value::String(s) => apply_one_type(s, out),
        Value::Array(arr) => {
            let mut names = Vec::new();
            for item in arr {
                if let Some(s) = item.as_str() {
                    if s.eq_ignore_ascii_case("null") {
                        out.insert("nullable".into(), Value::Bool(true));
                    } else {
                        names.push(s);
                    }
                }
            }
            match names.as_slice() {
                [] => {}
                [one] => apply_one_type(one, out),
                _ => {
                    let schemas: Vec<Value> = names
                        .iter()
                        .filter_map(|n| gemini_type_name(n).map(|ty| json!({"type": ty})))
                        .collect();
                    flatten_any_of(schemas, out);
                }
            }
        }
        _ => {}
    }
}

fn apply_one_type(raw: &str, out: &mut Map<String, Value>) {
    if raw.eq_ignore_ascii_case("null") {
        out.insert("nullable".into(), Value::Bool(true));
        return;
    }
    if let Some(ty) = gemini_type_name(raw) {
        out.insert("type".into(), Value::String(ty.to_string()));
    }
}

fn gemini_type_name(raw: &str) -> Option<&'static str> {
    Some(match raw.to_ascii_uppercase().as_str() {
        "STRING" => "STRING",
        "NUMBER" => "NUMBER",
        "INTEGER" => "INTEGER",
        "BOOLEAN" => "BOOLEAN",
        "ARRAY" => "ARRAY",
        "OBJECT" => "OBJECT",
        _ => return None,
    })
}

fn apply_enum_field(map: &Map<String, Value>, out: &mut Map<String, Value>) {
    let Some(arr) = map.get("enum").and_then(Value::as_array) else {
        return;
    };
    let mut vals = Vec::new();
    for item in arr {
        if item.is_null() {
            out.insert("nullable".into(), Value::Bool(true));
            continue;
        }
        if let Some(s) = item.as_str() {
            vals.push(Value::String(s.to_string()));
        } else {
            vals.push(Value::String(item.to_string()));
        }
    }
    if !vals.is_empty() {
        out.insert("enum".into(), Value::Array(vals));
    }
}

fn apply_const_field(map: &Map<String, Value>, out: &mut Map<String, Value>) {
    let Some(c) = map.get("const") else {
        return;
    };
    if out.contains_key("enum") {
        return;
    }
    if c.is_null() {
        out.insert("nullable".into(), Value::Bool(true));
        return;
    }
    let s = c
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| c.to_string());
    out.insert("enum".into(), json!([s]));
}

fn finish_object(mut obj: Map<String, Value>) -> Value {
    isolate_any_of(&mut obj);
    Value::Object(obj)
}

/// Gemini Schema proto: when `anyOf` is set it must be the only field.
fn isolate_any_of(obj: &mut Map<String, Value>) {
    let Some(Value::Array(branches)) = obj.remove("anyOf") else {
        return;
    };
    if obj.is_empty() {
        obj.insert("anyOf".into(), Value::Array(branches));
        return;
    }
    let siblings = std::mem::take(obj);
    let merged = branches
        .into_iter()
        .map(|branch| {
            let mut branch_obj = match branch {
                Value::Object(m) => m,
                other => {
                    let mut wrap = Map::new();
                    wrap.insert("enum".into(), json!([other]));
                    wrap
                }
            };
            for (k, v) in &siblings {
                branch_obj.entry(k.clone()).or_insert_with(|| v.clone());
            }
            Value::Object(branch_obj)
        })
        .collect();
    obj.insert("anyOf".into(), Value::Array(merged));
}

fn flatten_any_of(items: Vec<Value>, out: &mut Map<String, Value>) {
    let mut nullable = out
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut rest = Vec::new();
    for item in items {
        if is_null_schema(&item) {
            nullable = true;
        } else {
            rest.push(item);
        }
    }
    if nullable {
        out.insert("nullable".into(), Value::Bool(true));
    }
    match rest.len() {
        0 => {}
        1 => {
            if let Some(Value::Object(single)) = rest.pop() {
                for (k, v) in single {
                    out.entry(k).or_insert(v);
                }
            }
        }
        _ => {
            out.insert("anyOf".into(), Value::Array(rest));
        }
    }
}

fn is_null_schema(item: &Value) -> bool {
    let Some(obj) = item.as_object() else {
        return false;
    };
    if obj.get("type").and_then(Value::as_str) == Some("NULL") {
        return obj
            .keys()
            .all(|k| k == "type" || k == "nullable" || k == "description" || k == "title");
    }
    obj.get("nullable").and_then(Value::as_bool) == Some(true)
        && obj.get("type").is_none()
        && obj
            .keys()
            .all(|k| k == "nullable" || k == "description" || k == "title")
}

fn copy_passthrough_fields(map: &Map<String, Value>, out: &mut Map<String, Value>) {
    for key in [
        "description",
        "title",
        "format",
        "pattern",
        "default",
        "example",
        "nullable",
        "propertyOrdering",
        "required",
        "minItems",
        "maxItems",
        "minLength",
        "maxLength",
        "minProperties",
        "maxProperties",
        "minimum",
        "maximum",
    ] {
        if let Some(v) = map.get(key)
            && !out.contains_key(key)
        {
            out.insert(key.to_string(), v.clone());
        }
    }
}

fn merge_schema_objects(into: &mut Map<String, Value>, from: Value) {
    let Value::Object(from) = from else {
        return;
    };
    for (k, v) in from {
        match k.as_str() {
            "properties" => {
                let dest = into.entry("properties").or_insert_with(|| json!({}));
                if let (Some(d), Some(s)) = (dest.as_object_mut(), v.as_object()) {
                    for (pk, pv) in s {
                        d.insert(pk.clone(), pv.clone());
                    }
                }
            }
            "required" => {
                let dest = into.entry("required").or_insert_with(|| json!([]));
                if let (Some(d), Some(s)) = (dest.as_array_mut(), v.as_array()) {
                    for item in s {
                        if !d.contains(item) {
                            d.push(item.clone());
                        }
                    }
                }
            }
            "anyOf" => {
                let dest = into.entry("anyOf").or_insert_with(|| json!([]));
                if let (Some(d), Some(s)) = (dest.as_array_mut(), v.as_array()) {
                    d.extend(s.iter().cloned());
                }
            }
            _ => {
                into.insert(k, v);
            }
        }
    }
}

#[cfg(test)]
mod schema_tests {
    use super::json_schema_to_gemini_schema;
    use serde_json::{Value, json};

    #[test]
    fn strips_json_schema_keywords_and_uppercases_types() {
        let converted = json_schema_to_gemini_schema(&json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "additionalProperties": false,
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "file" },
                "timeout": { "type": ["integer", "null"], "default": 120000 },
                "status": {
                    "type": ["string", "null"],
                    "enum": ["pending", "done", null]
                }
            }
        }));
        assert!(converted.get("$schema").is_none());
        assert!(converted.get("additionalProperties").is_none());
        assert_eq!(converted.get("type"), Some(&json!("OBJECT")));
        let props = converted.get("properties").and_then(Value::as_object);
        let path = props.and_then(|p| p.get("path"));
        let timeout = props.and_then(|p| p.get("timeout"));
        let status = props.and_then(|p| p.get("status"));
        assert_eq!(path.and_then(|p| p.get("type")), Some(&json!("STRING")));
        assert_eq!(timeout.and_then(|p| p.get("type")), Some(&json!("INTEGER")));
        assert_eq!(timeout.and_then(|p| p.get("nullable")), Some(&json!(true)));
        assert_eq!(status.and_then(|p| p.get("type")), Some(&json!("STRING")));
        assert_eq!(status.and_then(|p| p.get("nullable")), Some(&json!(true)));
        assert_eq!(
            status.and_then(|p| p.get("enum")),
            Some(&json!(["pending", "done"]))
        );
        assert!(
            status
                .and_then(|p| p.get("enum"))
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().all(|v| !v.is_null()))
        );
    }

    #[test]
    fn resolves_defs_refs_and_merges_all_of() {
        let converted = json_schema_to_gemini_schema(&json!({
            "type": "object",
            "properties": {
                "item": { "$ref": "#/$defs/Item" }
            },
            "$defs": {
                "Item": {
                    "allOf": [
                        { "type": "object", "properties": { "id": { "type": "string" } } }
                    ],
                    "description": "an item"
                }
            }
        }));
        assert!(converted.get("$defs").is_none());
        let item = converted
            .get("properties")
            .and_then(Value::as_object)
            .and_then(|p| p.get("item"));
        assert_eq!(item.and_then(|i| i.get("type")), Some(&json!("OBJECT")));
        assert_eq!(
            item.and_then(|i| i.get("properties"))
                .and_then(Value::as_object)
                .and_then(|p| p.get("id"))
                .and_then(|id| id.get("type")),
            Some(&json!("STRING"))
        );
        assert_eq!(
            item.and_then(|i| i.get("description")),
            Some(&json!("an item"))
        );
    }

    fn assert_any_of_is_sole_field(schema: &Value) {
        if let Some(obj) = schema.as_object() {
            if obj.contains_key("anyOf") {
                assert_eq!(
                    obj.len(),
                    1,
                    "Gemini rejects sibling fields next to anyOf: {schema}"
                );
            }
            for value in obj.values() {
                assert_any_of_is_sole_field(value);
            }
        } else if let Some(arr) = schema.as_array() {
            for value in arr {
                assert_any_of_is_sole_field(value);
            }
        }
    }

    #[test]
    fn any_of_cannot_sit_beside_other_schema_fields() {
        let converted = json_schema_to_gemini_schema(&json!({
            "description": "number or string",
            "anyOf": [
                { "type": "string" },
                { "type": "integer" }
            ]
        }));
        assert_any_of_is_sole_field(&converted);
        let branches = converted
            .get("anyOf")
            .and_then(Value::as_array)
            .expect("anyOf");
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches.first().and_then(|b| b.get("type")),
            Some(&json!("STRING"))
        );
        assert_eq!(
            branches.first().and_then(|b| b.get("description")),
            Some(&json!("number or string"))
        );
    }

    #[test]
    fn use_tool_oneof_becomes_any_of_only_object() {
        let converted = json_schema_to_gemini_schema(&json!({
            "type": "object",
            "properties": {
                "tool_name": { "type": "string" },
                "tool_input": { "type": "object" },
                "tool_input_file": { "type": "string" },
                "file": { "type": "string" }
            },
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "tool_name": { "type": "string" },
                        "tool_input": { "type": "object" }
                    },
                    "required": ["tool_name", "tool_input"]
                },
                {
                    "type": "object",
                    "properties": { "file": { "type": "string" } },
                    "required": ["file"]
                }
            ]
        }));
        assert_any_of_is_sole_field(&converted);
        let branches = converted
            .get("anyOf")
            .and_then(Value::as_array)
            .expect("anyOf");
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches.first().and_then(|b| b.get("type")),
            Some(&json!("OBJECT"))
        );
        assert!(
            branches
                .first()
                .and_then(|b| b.get("required"))
                .and_then(Value::as_array)
                .is_some_and(|r| r.iter().any(|v| v.as_str() == Some("tool_name")))
        );
    }
}
