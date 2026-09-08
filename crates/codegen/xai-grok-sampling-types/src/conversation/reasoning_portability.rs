//! Outbound thinking/reasoning is minted per wire protocol and rejected if replayed
//! onto another (`Invalid signature in thinking block`, `Invalid 'input[N].id': ''`).
//! Conversation history is left intact; converters omit foreign payloads at request build.

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

/// Whether this item can be sent on `/v1/responses` without a foreign-payload 400.
pub(crate) fn reasoning_is_portable_to_responses(r: &ReasoningItem) -> bool {
    let id = r.id.as_str();
    let enc = blob(r);
    if id.starts_with(OPENAI_REASONING_ID_PREFIX)
        || id.starts_with(XAI_ENC_PREFIX)
        || enc.starts_with(OPENAI_ENC_PREFIX)
        || enc.starts_with(XAI_ENC_PREFIX)
    {
        return true;
    }
    if enc.starts_with(ANTHROPIC_SIG_PREFIX) {
        return false;
    }
    // Synthesized plaintext thinking (empty id, no blob) is not a Responses item.
    // Claude thinking is the `CA…` case above. Other encrypted blobs with an
    // empty id still go out; the API may assign identity.
    !(id.is_empty() && enc.is_empty())
}

/// Whether this item can be sent on `/v1/messages` as a thinking block.
///
/// Anthropic verifies `signature`. OpenAI `gAAAAA…` and xAI `tco_…` blobs fail
/// with `Invalid signature in thinking block`. Empty signature is synthesized
/// plaintext thinking and is left for the existing Messages path.
pub(crate) fn reasoning_is_portable_to_messages(r: &ReasoningItem) -> bool {
    let sig = blob(r);
    !sig.starts_with(OPENAI_ENC_PREFIX) && !sig.starts_with(XAI_ENC_PREFIX)
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
    fn responses_keeps_openai_and_xai_drops_claude() {
        assert!(reasoning_is_portable_to_responses(&item(
            "rs_abc",
            Some("gAAAAAencrypted")
        )));
        assert!(reasoning_is_portable_to_responses(&item(
            "tco_res-uuid",
            Some("tco_SEALED")
        )));
        assert!(reasoning_is_portable_to_responses(&item(
            "r1",
            Some("enc_secret_reasoning_chain")
        )));
        assert!(!reasoning_is_portable_to_responses(&item(
            "",
            Some("CAsignature")
        )));
        assert!(!reasoning_is_portable_to_responses(&item("", None)));
        assert!(reasoning_is_portable_to_responses(&item(
            "",
            Some("enc_hidden_thoughts")
        )));
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
    }
}
