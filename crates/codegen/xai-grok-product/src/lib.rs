//! Product identity for the unofficial Simplified Chinese community overlay.
//!
//! Keep distribution identity separate from UI localization. Protocol names,
//! server endpoints, model IDs, tool names, and wire fields must not depend on
//! this crate.
//!
//! Auto-update stays on the official grok-build channel. This crate does not
//! redirect or disable vendor updates.

#![forbid(unsafe_code)]

/// Compile-time privacy switch, mirrored from `xai-grok-version`'s `privacy`
/// feature (default-on in this overlay).
pub const PRIVACY_BUILD: bool = xai_grok_version::PRIVACY_BUILD;

/// Stable machine-readable identity used by packaging and release metadata.
pub const PRODUCT_ID: &str = "grok-build-zh";
/// Human-readable product name for client-owned UI chrome.
pub const DISPLAY_NAME: &str = if PRIVACY_BUILD {
    "Grok Build 中文社区版（隐私构建）"
} else {
    "Grok Build 中文社区版"
};
/// Command the user types for this overlay. Official grok-build keeps `grok`.
pub const CLI_NAME: &str = "grokx";
/// Filename of the official managed install under `$GROK_HOME/bin`.
/// This overlay does not occupy that name.
pub const OFFICIAL_CLI_NAME: &str = "grok";
/// Shared per-user data directory, relative to the user's home directory.
///
/// The official and Simplified Chinese executables intentionally use the same
/// sessions, credentials, configuration, plugins, caches, and local state.
pub const DATA_DIR_NAME: &str = ".grok";
/// Shared user-data override used by both the official and Chinese executables.
pub const HOME_ENV: &str = "GROK_HOME";
/// Distribution-specific UI locale override.
pub const LOCALE_ENV: &str = "GROK_ZH_LOCALE";
/// Default UI locale for this distribution.
pub const DEFAULT_UI_LOCALE: &str = "zh-CN";
/// Independently versioned display translations. Not an update channel.
pub const COMMUNITY_ANNOUNCEMENTS_BASE_URL: &str = "https://raw.githubusercontent.com/Catapult291/GrokZen/refs/heads/zh-dev/community/announcements";
/// Official changelog CDN remains allowed; this overlay does not redirect updates.
pub const OFFICIAL_CHANGELOG_SOURCE_ALLOWED: bool = true;

/// Executable filename for this overlay.
pub const fn executable_name() -> &'static str {
    if cfg!(windows) { "grokx.exe" } else { CLI_NAME }
}

/// Official managed-install filename (`$GROK_HOME/bin/grok`). Auto-update still
/// targets this path; this overlay's `grokx` binary is not that install.
pub const fn official_executable_name() -> &'static str {
    if cfg!(windows) {
        "grok.exe"
    } else {
        OFFICIAL_CLI_NAME
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_build_suffixes_display_name() {
        assert_eq!(
            PRIVACY_BUILD,
            xai_grok_version::PRIVACY_BUILD,
            "product policy must mirror the compile-time privacy switch"
        );
        assert_eq!(
            DISPLAY_NAME,
            if PRIVACY_BUILD {
                "Grok Build 中文社区版（隐私构建）"
            } else {
                "Grok Build 中文社区版"
            }
        );
        assert!(
            !PRIVACY_BUILD || DISPLAY_NAME.contains("隐私构建"),
            "privacy builds must advertise the hardened posture"
        );
    }

    #[test]
    fn community_ui_identity_uses_the_shared_official_data_home() {
        assert_eq!(PRODUCT_ID, "grok-build-zh");
        assert_eq!(DATA_DIR_NAME, ".grok");
        assert_eq!(HOME_ENV, "GROK_HOME");
        assert_eq!(LOCALE_ENV, "GROK_ZH_LOCALE");
        assert_eq!(DEFAULT_UI_LOCALE, "zh-CN");
        assert_eq!(
            executable_name(),
            if cfg!(windows) { "grokx.exe" } else { "grokx" }
        );
        assert_eq!(
            official_executable_name(),
            if cfg!(windows) { "grok.exe" } else { "grok" }
        );
        assert_ne!(CLI_NAME, OFFICIAL_CLI_NAME);
    }
}
