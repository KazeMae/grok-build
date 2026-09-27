//! Outbound thinking/reasoning is minted per wire protocol and rejected if replayed
//! onto another (`Invalid signature in thinking block`, `Invalid 'input[N].id': ''`,
//! `Could not decrypt the provided encrypted_content`).
//! Conversation history is left intact; converters omit foreign payloads at request build.

use crate::gemini::GEMINI_THOUGHT_SIG_PREFIX;
use crate::rs::ReasoningItem;

/// OpenAI Responses encrypted reasoning (`gpt-6-astra` and similar).
const OPENAI_ENC_PREFIX: &str = "gAAAAA";
/// OpenAI Responses reasoning item ids.
const OPENAI_REASONING_ID_PREFIX: &str = "rs_";
/// xAI Responses encrypted reasoning / tool-call blobs.
const XAI_ENC_PREFIX: &str = "tco_";
/// Anthropic Messages thinking signatures.
const ANTHROPIC_SIG_PREFIX: &str = "CA";

fn blob(r: &ReasoningItem) -> &str {
    r.encrypted_content.as_deref().unwrap_or("")
}

fn is_openai_reasoning(r: &ReasoningItem) -> bool {
    r.id.starts_with(OPENAI_REASONING_ID_PREFIX) || blob(r).starts_with(OPENAI_ENC_PREFIX)
}

fn is_xai_reasoning(r: &ReasoningItem) -> bool {
    r.id.starts_with(XAI_ENC_PREFIX) || blob(r).starts_with(XAI_ENC_PREFIX)
}

fn dest_name_contains(model: Option<&str>, needle: &str) -> bool {
    model.is_some_and(|m| m.to_ascii_lowercase().contains(needle))
}

/// Whether this item can be sent on `/v1/responses` without a foreign-payload 400.
///
/// OpenAI (`gAAAAA` / `rs_`) and xAI (`tco_`) blobs only round-trip to the
/// provider that minted them. `dest_model` is the request's target slug
/// (`gpt-6-astra`, `uniapi-gpt-6-astra`, `grok-4.7`, …).
pub(crate) fn reasoning_is_portable_to_responses(
    r: &ReasoningItem,
    dest_model: Option<&str>,
) -> bool {
    if is_openai_reasoning(r) {
        return dest_name_contains(dest_model, "gpt");
    }
    if is_xai_reasoning(r) {
        return dest_name_contains(dest_model, "grok");
    }
    let enc = blob(r);
    if enc.starts_with(ANTHROPIC_SIG_PREFIX) || enc.starts_with(GEMINI_THOUGHT_SIG_PREFIX) {
        return false;
    }
    // Synthesized plaintext thinking (empty id, no blob) is not a Responses item.
    // Claude thinking is the `CA…` case above. Other encrypted blobs with an
    // empty id still go out; the API may assign identity.
    !(r.id.is_empty() && enc.is_empty())
}

/// Whether this item can be sent on `/v1/messages` as a thinking block.
///
/// Anthropic verifies `signature`. OpenAI `gAAAAA…` and xAI `tco_…` blobs fail
/// with `Invalid signature in thinking block`. Empty signature is synthesized
/// plaintext thinking and is left for the existing Messages path.
pub(crate) fn reasoning_is_portable_to_messages(r: &ReasoningItem) -> bool {
    let sig = blob(r);
    !sig.starts_with(OPENAI_ENC_PREFIX)
        && !sig.starts_with(XAI_ENC_PREFIX)
        && !sig.starts_with(GEMINI_THOUGHT_SIG_PREFIX)
}

/// Whether this item can be sent on Gemini `generateContent` as a thought / thoughtSignature.
pub(crate) fn reasoning_is_portable_to_gemini(r: &ReasoningItem) -> bool {
    let sig = blob(r);
    if sig.starts_with(OPENAI_ENC_PREFIX)
        || sig.starts_with(XAI_ENC_PREFIX)
        || sig.starts_with(ANTHROPIC_SIG_PREFIX)
    {
        return false;
    }
    sig.starts_with(GEMINI_THOUGHT_SIG_PREFIX) || (sig.is_empty() && !r.summary.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, enc: Option<&str>) -> ReasoningItem {
        ReasoningItem {
            id: id.to_string(),
            summary: vec![],
            content: None,
            encrypted_content: enc.map(str::to_owned),
            status: None,
        }
    }

    #[test]
    fn responses_keeps_openai_only_for_gpt_destination() {
        let openai = item("rs_abc", Some("gAAAAAencrypted"));
        let xai = item("tco_res-uuid", Some("tco_SEALED"));
        let generic = item("r1", Some("enc_secret_reasoning_chain"));
        let claude = item("", Some("CAsignature"));
        let empty = item("", None);
        let other_enc = item("", Some("enc_hidden_thoughts"));
        let gemini = item("", Some("gsig:GEMINI_SIG"));

        assert!(reasoning_is_portable_to_responses(
            &openai,
            Some("gpt-6-astra")
        ));
        assert!(reasoning_is_portable_to_responses(
            &openai,
            Some("uniapi-gpt-6-astra")
        ));
        assert!(!reasoning_is_portable_to_responses(
            &openai,
            Some("grok-4.7")
        ));
        assert!(!reasoning_is_portable_to_responses(&openai, None));

        assert!(reasoning_is_portable_to_responses(
            &xai,
            Some("grok-4.7-build-fast")
        ));
        assert!(!reasoning_is_portable_to_responses(
            &xai,
            Some("gpt-6-astra")
        ));
        assert!(!reasoning_is_portable_to_responses(&xai, None));

        for dest in [Some("grok-4.7"), Some("gpt-6-astra"), None] {
            assert!(reasoning_is_portable_to_responses(&generic, dest));
            assert!(!reasoning_is_portable_to_responses(&claude, dest));
            assert!(!reasoning_is_portable_to_responses(&empty, dest));
            assert!(reasoning_is_portable_to_responses(&other_enc, dest));
            assert!(!reasoning_is_portable_to_responses(&gemini, dest));
        }
    }

    #[test]
    fn messages_drops_openai_and_xai_keeps_claude_and_synthesized() {
        assert!(!reasoning_is_portable_to_messages(&item(
            "rs_abc",
            Some("gAAAAAencrypted")
        )));
        assert!(!reasoning_is_portable_to_messages(&item(
            "tco_res-uuid",
            Some("tco_SEALED")
        )));
        assert!(reasoning_is_portable_to_messages(&item(
            "",
            Some("CAsignature")
        )));
        assert!(reasoning_is_portable_to_messages(&item("", None)));
        assert!(!reasoning_is_portable_to_messages(&item(
            "",
            Some("gsig:GEMINI_SIG")
        )));
    }

    #[test]
    fn gemini_keeps_gsig_and_plain_summary_drops_foreign() {
        assert!(reasoning_is_portable_to_gemini(&item(
            "",
            Some("gsig:GEMINI_SIG")
        )));
        assert!(!reasoning_is_portable_to_gemini(&item(
            "rs_abc",
            Some("gAAAAAencrypted")
        )));
        assert!(!reasoning_is_portable_to_gemini(&item(
            "",
            Some("CAsignature")
        )));
        assert!(!reasoning_is_portable_to_gemini(&item("", None)));
    }
}
