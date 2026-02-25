//! Gemini authentication type detection
//!
//! Detects whether a Gemini provider uses PackyCode API Key, Google OAuth, or generic API Key.

use crate::error::AppError;
use crate::provider::Provider;

/// Gemini authentication type enumeration
///
/// Used to optimize performance by avoiding repeated provider type detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeminiAuthType {
    /// PackyCode provider (uses API Key)
    Packycode,
    /// Google Official (uses OAuth)
    GoogleOfficial,
    /// Generic Gemini provider (uses API Key)
    Generic,
}

// Partner Promotion Key constants
const PACKYCODE_PARTNER_KEY: &str = "packycode";
const GOOGLE_OFFICIAL_PARTNER_KEY: &str = "google-official";
const GOOGLE_GENERATIVE_LANGUAGE_HOST: &str = "generativelanguage.googleapis.com";

// PackyCode keyword constants
const PACKYCODE_KEYWORDS: [&str; 3] = ["packycode", "packyapi", "packy"];

fn has_non_empty_env_value(provider: &Provider, key: &str) -> bool {
    provider
        .settings_config
        .get("env")
        .and_then(|v| v.as_object())
        .and_then(|env| env.get(key))
        .and_then(|v| v.as_str())
        .is_some_and(|value| !value.trim().is_empty())
}

fn has_any_gemini_api_key(provider: &Provider) -> bool {
    has_non_empty_env_value(provider, "GEMINI_API_KEY")
        || has_non_empty_env_value(provider, "GOOGLE_API_KEY")
}

fn has_oauth_selected_type(provider: &Provider) -> bool {
    provider
        .settings_config
        .pointer("/config/security/auth/selectedType")
        .and_then(|v| v.as_str())
        == Some("oauth-personal")
}

fn has_oauth_auth_files(provider: &Provider) -> bool {
    provider
        .settings_config
        .pointer("/authFiles/enabled")
        .and_then(|v| v.as_bool())
        == Some(true)
        && !has_any_gemini_api_key(provider)
}

fn is_official_category_without_api_key(provider: &Provider) -> bool {
    provider
        .category
        .as_deref()
        .is_some_and(|category| category.eq_ignore_ascii_case("official"))
        && !has_any_gemini_api_key(provider)
}

fn get_gemini_base_url(provider: &Provider) -> Option<&str> {
    provider
        .settings_config
        .pointer("/env/GOOGLE_GEMINI_BASE_URL")
        .and_then(|v| v.as_str())
}

fn is_google_official_base_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }

    lower.contains(GOOGLE_GENERATIVE_LANGUAGE_HOST) || lower.contains("ai.google.dev")
}

fn is_likely_google_oauth_without_api_key(provider: &Provider) -> bool {
    if has_any_gemini_api_key(provider) {
        return false;
    }

    match get_gemini_base_url(provider) {
        None => true,
        Some(url) if url.trim().is_empty() => true,
        Some(url) => is_google_official_base_url(url),
    }
}

/// Detect Gemini provider authentication type
///
/// One-time detection to avoid repeated calls to `is_packycode_gemini` and `is_google_official_gemini`.
///
/// # Returns
///
/// - `GeminiAuthType::GoogleOfficial`: Google official, uses OAuth
/// - `GeminiAuthType::Packycode`: PackyCode provider, uses API Key
/// - `GeminiAuthType::Generic`: Other generic providers, uses API Key
pub(crate) fn detect_gemini_auth_type(provider: &Provider) -> GeminiAuthType {
    // Priority 1: Check partner_promotion_key (most reliable)
    if let Some(key) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.partner_promotion_key.as_deref())
    {
        if key.eq_ignore_ascii_case(GOOGLE_OFFICIAL_PARTNER_KEY) {
            return GeminiAuthType::GoogleOfficial;
        }
        if key.eq_ignore_ascii_case(PACKYCODE_PARTNER_KEY) {
            return GeminiAuthType::Packycode;
        }
    }

    // Priority 2: Check explicit OAuth markers in config
    if has_oauth_selected_type(provider) || has_oauth_auth_files(provider) {
        return GeminiAuthType::GoogleOfficial;
    }

    // Priority 3: Treat official category without API key as Google OAuth
    if is_official_category_without_api_key(provider) {
        return GeminiAuthType::GoogleOfficial;
    }

    // Priority 4: Fallback heuristic for imported OAuth profiles.
    // If there is no API key and endpoint is empty/Google official, treat as OAuth.
    if is_likely_google_oauth_without_api_key(provider) {
        return GeminiAuthType::GoogleOfficial;
    }

    // Priority 5: Check Google Official (name matching)
    let name_lower = provider.name.to_ascii_lowercase();
    if name_lower == "google" || name_lower.starts_with("google ") {
        return GeminiAuthType::GoogleOfficial;
    }

    // Priority 6: Check PackyCode keywords
    if contains_packycode_keyword(&provider.name) {
        return GeminiAuthType::Packycode;
    }

    if let Some(site) = provider.website_url.as_deref() {
        if contains_packycode_keyword(site) {
            return GeminiAuthType::Packycode;
        }
    }

    if let Some(base_url) = provider
        .settings_config
        .pointer("/env/GOOGLE_GEMINI_BASE_URL")
        .and_then(|v| v.as_str())
    {
        if contains_packycode_keyword(base_url) {
            return GeminiAuthType::Packycode;
        }
    }

    GeminiAuthType::Generic
}

/// Check if string contains PackyCode related keywords (case-insensitive)
///
/// Keyword list: ["packycode", "packyapi", "packy"]
fn contains_packycode_keyword(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    PACKYCODE_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

/// Detect if provider is Google Official Gemini (uses OAuth authentication)
///
/// Google Official Gemini uses OAuth personal authentication, no API Key needed.
///
/// This is a convenience wrapper around `detect_gemini_auth_type`.
pub(crate) fn is_google_official_gemini(provider: &Provider) -> bool {
    detect_gemini_auth_type(provider) == GeminiAuthType::GoogleOfficial
}

/// Ensure Google Official Gemini provider security flag is correctly set (OAuth mode)
///
/// Google Official Gemini uses OAuth personal authentication, no API Key needed.
///
/// # What it does
///
/// Writes to **`~/.gemini/settings.json`** (Gemini client config).
///
/// # Value set
///
/// ```json
/// {
///   "security": {
///     "auth": {
///       "selectedType": "oauth-personal"
///     }
///   }
/// }
/// ```
///
/// # OAuth authentication flow
///
/// 1. User switches to Google Official provider
/// 2. CC-Switch sets `selectedType = "oauth-personal"`
/// 3. User's first use of Gemini CLI will auto-open browser for OAuth login
/// 4. After successful login, credentials saved in Gemini credential store
/// 5. Subsequent requests auto-use saved credentials
///
/// # Error handling
///
/// If provider is not Google Official, function returns `Ok(())` immediately without any operation.
pub(crate) fn ensure_google_oauth_security_flag(provider: &Provider) -> Result<(), AppError> {
    if !is_google_official_gemini(provider) {
        return Ok(());
    }

    // Write to Gemini directory settings.json (~/.gemini/settings.json)
    use crate::gemini_config::write_google_oauth_settings;
    write_google_oauth_settings()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn build_provider(name: &str, category: Option<&str>, settings: serde_json::Value) -> Provider {
        let mut provider = Provider::with_id(
            "gemini-test".to_string(),
            name.to_string(),
            settings,
            Some("https://example.com".to_string()),
        );
        provider.category = category.map(|value| value.to_string());
        provider
    }

    #[test]
    fn detect_google_official_from_oauth_selected_type_even_with_custom_name() {
        let provider = build_provider(
            "Imported Config",
            Some("custom"),
            json!({
                "env": {},
                "config": {
                    "security": {
                        "auth": {
                            "selectedType": "oauth-personal"
                        }
                    }
                }
            }),
        );

        assert_eq!(
            detect_gemini_auth_type(&provider),
            GeminiAuthType::GoogleOfficial
        );
    }

    #[test]
    fn detect_google_official_from_auth_files_without_api_key() {
        let provider = build_provider(
            "Imported Config",
            Some("custom"),
            json!({
                "env": {},
                "authFiles": {
                    "enabled": true
                }
            }),
        );

        assert_eq!(
            detect_gemini_auth_type(&provider),
            GeminiAuthType::GoogleOfficial
        );
    }

    #[test]
    fn auth_files_with_api_key_is_not_forced_to_google_official() {
        let provider = build_provider(
            "Imported Config",
            Some("custom"),
            json!({
                "env": {
                    "GEMINI_API_KEY": "sk-test"
                },
                "authFiles": {
                    "enabled": true
                }
            }),
        );

        assert_ne!(
            detect_gemini_auth_type(&provider),
            GeminiAuthType::GoogleOfficial
        );
    }

    #[test]
    fn detect_google_official_when_no_api_key_and_empty_base_url() {
        let provider = build_provider(
            "Imported Config",
            Some("custom"),
            json!({
                "env": {
                    "GEMINI_MODEL": "gemini-3-pro-preview"
                },
                "config": {}
            }),
        );

        assert_eq!(
            detect_gemini_auth_type(&provider),
            GeminiAuthType::GoogleOfficial
        );
    }

    #[test]
    fn detect_google_official_when_no_api_key_and_google_base_url() {
        let provider = build_provider(
            "Imported Config",
            Some("custom"),
            json!({
                "env": {
                    "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com",
                    "GEMINI_MODEL": "gemini-3-pro-preview"
                },
                "config": {}
            }),
        );

        assert_eq!(
            detect_gemini_auth_type(&provider),
            GeminiAuthType::GoogleOfficial
        );
    }

    #[test]
    fn no_api_key_with_custom_base_url_is_not_google_official() {
        let provider = build_provider(
            "Imported Config",
            Some("custom"),
            json!({
                "env": {
                    "GOOGLE_GEMINI_BASE_URL": "https://custom-gateway.example.com",
                    "GEMINI_MODEL": "gemini-3-pro-preview"
                },
                "config": {}
            }),
        );

        assert_eq!(detect_gemini_auth_type(&provider), GeminiAuthType::Generic);
    }
}
