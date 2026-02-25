use serde_json::json;

use cc_switch_lib::{
    get_claude_settings_path, read_json_file, write_codex_live_atomic, AppError, AppType, McpApps,
    McpServer, MultiAppConfig, Provider, ProviderMeta, ProviderService, AppSettings, update_settings,
};

#[path = "support.rs"]
mod support;
use support::{
    create_test_state, create_test_state_with_config, ensure_test_home, reset_test_fs, test_mutex,
};

fn sanitize_provider_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            _ => c,
        })
        .collect::<String>()
        .to_lowercase()
}

#[test]
fn provider_service_switch_codex_updates_live_and_config() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let legacy_auth = json!({ "OPENAI_API_KEY": "legacy-key" });
    let legacy_config = r#"[mcp_servers.legacy]
type = "stdio"
command = "echo"
"#;
    write_codex_live_atomic(&legacy_auth, Some(legacy_config))
        .expect("seed existing codex live config");

    let mut initial_config = MultiAppConfig::default();
    {
        let manager = initial_config
            .get_manager_mut(&AppType::Codex)
            .expect("codex manager");
        manager.current = "old-provider".to_string();
        manager.providers.insert(
            "old-provider".to_string(),
            Provider::with_id(
                "old-provider".to_string(),
                "Legacy".to_string(),
                json!({
                    "auth": {"OPENAI_API_KEY": "stale"},
                    "config": "stale-config"
                }),
                None,
            ),
        );
        manager.providers.insert(
            "new-provider".to_string(),
            Provider::with_id(
                "new-provider".to_string(),
                "Latest".to_string(),
                json!({
                    "auth": {"OPENAI_API_KEY": "fresh-key"},
                    "config": r#"[mcp_servers.latest]
type = "stdio"
command = "say"
"#
                }),
                None,
            ),
        );
    }

    // 使用新的统一 MCP 结构（v3.7.0+）
    let servers = initial_config
        .mcp
        .servers
        .get_or_insert_with(Default::default);
    servers.insert(
        "echo-server".into(),
        McpServer {
            id: "echo-server".into(),
            name: "Echo Server".into(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                codex: true,
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    );

    let state = create_test_state_with_config(&initial_config).expect("create test state");

    ProviderService::switch(&state, AppType::Codex, "new-provider")
        .expect("switch provider should succeed");

    let auth_value: serde_json::Value =
        read_json_file(&cc_switch_lib::get_codex_auth_path()).expect("read auth.json");
    assert_eq!(
        auth_value.get("OPENAI_API_KEY").and_then(|v| v.as_str()),
        Some("fresh-key"),
        "live auth.json should reflect new provider"
    );

    let config_text =
        std::fs::read_to_string(cc_switch_lib::get_codex_config_path()).expect("read config.toml");
    // With partial merge, only key fields (model, provider, model_providers) are
    // merged into config.toml. The existing MCP section should be preserved.
    // MCP sync from DB is handled separately (at startup or explicit sync).
    assert!(
        config_text.contains("mcp_servers.legacy"),
        "config.toml should preserve existing MCP servers after partial merge"
    );

    let current_id = state
        .db
        .get_current_provider(AppType::Codex.as_str())
        .expect("read current provider after switch");
    assert_eq!(
        current_id.as_deref(),
        Some("new-provider"),
        "current provider updated"
    );

    let providers = state
        .db
        .get_all_providers(AppType::Codex.as_str())
        .expect("read providers after switch");

    let new_provider = providers.get("new-provider").expect("new provider exists");
    let new_config_text = new_provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    // provider 存储的是原始配置，不包含 MCP 同步后的内容
    assert!(
        new_config_text.contains("mcp_servers.latest"),
        "provider config should contain original MCP servers"
    );

    let legacy = providers
        .get("old-provider")
        .expect("legacy provider still exists");
    let legacy_auth_value = legacy
        .settings_config
        .get("auth")
        .and_then(|v| v.get("OPENAI_API_KEY"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(
        legacy_auth_value, "legacy-key",
        "previous provider should be backfilled with live auth"
    );
}

#[test]
fn switch_packycode_gemini_updates_security_selected_type() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "packy-gemini".to_string();
        manager.providers.insert(
            "packy-gemini".to_string(),
            Provider::with_id(
                "packy-gemini".to_string(),
                "PackyCode".to_string(),
                json!({
                    "env": {
                        "GEMINI_API_KEY": "pk-key",
                        "GOOGLE_GEMINI_BASE_URL": "https://www.packyapi.com"
                    }
                }),
                Some("https://www.packyapi.com".to_string()),
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::switch(&state, AppType::Gemini, "packy-gemini")
        .expect("switching to PackyCode Gemini should succeed");

    // Gemini security settings are written to ~/.gemini/settings.json, not ~/.cc-switch/settings.json
    let settings_path = home.join(".gemini").join("settings.json");
    assert!(
        settings_path.exists(),
        "Gemini settings.json should exist at {}",
        settings_path.display()
    );
    let raw = std::fs::read_to_string(&settings_path).expect("read gemini settings.json");
    let value: serde_json::Value =
        serde_json::from_str(&raw).expect("parse gemini settings.json after switch");

    assert_eq!(
        value
            .pointer("/security/auth/selectedType")
            .and_then(|v| v.as_str()),
        Some("gemini-api-key"),
        "PackyCode Gemini should set security.auth.selectedType"
    );
}

#[test]
fn packycode_partner_meta_triggers_security_flag_even_without_keywords() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "packy-meta".to_string();
        let mut provider = Provider::with_id(
            "packy-meta".to_string(),
            "Generic Gemini".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": "pk-meta",
                    "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com"
                }
            }),
            Some("https://example.com".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            partner_promotion_key: Some("packycode".to_string()),
            ..ProviderMeta::default()
        });
        manager.providers.insert("packy-meta".to_string(), provider);
    }

    let state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::switch(&state, AppType::Gemini, "packy-meta")
        .expect("switching to partner meta provider should succeed");

    // Gemini security settings are written to ~/.gemini/settings.json, not ~/.cc-switch/settings.json
    let settings_path = home.join(".gemini").join("settings.json");
    assert!(
        settings_path.exists(),
        "Gemini settings.json should exist at {}",
        settings_path.display()
    );
    let raw = std::fs::read_to_string(&settings_path).expect("read gemini settings.json");
    let value: serde_json::Value =
        serde_json::from_str(&raw).expect("parse gemini settings.json after switch");

    assert_eq!(
        value
            .pointer("/security/auth/selectedType")
            .and_then(|v| v.as_str()),
        Some("gemini-api-key"),
        "Partner meta should set security.auth.selectedType even without packy keywords"
    );
}

#[test]
fn switch_google_official_gemini_sets_oauth_security() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "google-official".to_string();
        let mut provider = Provider::with_id(
            "google-official".to_string(),
            "Google".to_string(),
            json!({
                "env": {
                    "GOOGLE_CLOUD_PROJECT": "oauth-project"
                }
            }),
            Some("https://ai.google.dev".to_string()),
        );
        provider.meta = Some(ProviderMeta {
            partner_promotion_key: Some("google-official".to_string()),
            ..ProviderMeta::default()
        });
        manager
            .providers
            .insert("google-official".to_string(), provider);
    }

    let state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::switch(&state, AppType::Gemini, "google-official")
        .expect("switching to Google official Gemini should succeed");

    // Gemini security settings are written to ~/.gemini/settings.json, not ~/.cc-switch/settings.json
    let gemini_settings = home.join(".gemini").join("settings.json");
    assert!(
        gemini_settings.exists(),
        "Gemini settings.json should exist at {}",
        gemini_settings.display()
    );
    let gemini_raw = std::fs::read_to_string(&gemini_settings).expect("read gemini settings");
    let gemini_value: serde_json::Value =
        serde_json::from_str(&gemini_raw).expect("parse gemini settings");

    assert_eq!(
        gemini_value
            .pointer("/security/auth/selectedType")
            .and_then(|v| v.as_str()),
        Some("oauth-personal"),
        "Gemini settings json should reflect oauth-personal for Google Official"
    );

    let env_path = home.join(".gemini").join(".env");
    assert!(
        env_path.exists(),
        "Gemini .env should exist at {}",
        env_path.display()
    );
    let env_raw = std::fs::read_to_string(&env_path).expect("read gemini .env");
    assert!(
        env_raw.contains("GOOGLE_CLOUD_PROJECT=oauth-project"),
        "Gemini OAuth profile should preserve non-api env vars"
    );
    assert!(
        !env_raw.contains("GEMINI_API_KEY="),
        "Gemini OAuth profile should not persist GEMINI_API_KEY"
    );
}

#[test]
fn switch_imported_like_gemini_without_api_key_uses_oauth_security() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "imported-gemini".to_string();
        manager.providers.insert(
            "imported-gemini".to_string(),
            Provider::with_id(
                "imported-gemini".to_string(),
                "Imported Config".to_string(),
                json!({
                    "env": {
                        "GEMINI_MODEL": "gemini-3-pro-preview",
                        "GOOGLE_CLOUD_PROJECT": "imported-project"
                    },
                    "config": {}
                }),
                None,
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");
    ProviderService::switch(&state, AppType::Gemini, "imported-gemini")
        .expect("switching imported Gemini profile without API key should succeed");

    let settings_path = home.join(".gemini").join("settings.json");
    assert!(
        settings_path.exists(),
        "Gemini settings.json should exist at {}",
        settings_path.display()
    );
    let raw = std::fs::read_to_string(&settings_path).expect("read gemini settings.json");
    let value: serde_json::Value =
        serde_json::from_str(&raw).expect("parse gemini settings.json after switch");

    assert_eq!(
        value
            .pointer("/security/auth/selectedType")
            .and_then(|v| v.as_str()),
        Some("oauth-personal"),
        "Imported-like OAuth Gemini profile should set oauth-personal"
    );

    let env_path = home.join(".gemini").join(".env");
    assert!(
        env_path.exists(),
        "Gemini .env should exist at {}",
        env_path.display()
    );
    let env_raw = std::fs::read_to_string(&env_path).expect("read gemini .env");
    assert!(
        env_raw.contains("GOOGLE_CLOUD_PROJECT=imported-project"),
        "Imported-like OAuth Gemini profile should keep custom .env entries"
    );
    assert!(
        env_raw.contains("GEMINI_MODEL=gemini-3-pro-preview"),
        "Imported-like OAuth Gemini profile should keep model in .env"
    );
}

#[test]
fn switch_gemini_backfill_preserves_profile_fields_when_switching_away() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "imported-x".to_string();
        manager.providers.insert(
            "imported-x".to_string(),
            Provider::with_id(
                "imported-x".to_string(),
                "Imported Config".to_string(),
                json!({
                    "env": {
                        "GOOGLE_CLOUD_PROJECT": "project-x",
                        "GOOGLE_CLOUD_LOCATION": "us-central1",
                        "GEMINI_MODEL": "gemini-3.1-pro-preview"
                    },
                    "config": {
                        "customSetting": "keep-me"
                    },
                    "authFiles": {
                        "enabled": true,
                        "googleAccounts": {
                            "accounts": [{ "email": "imported@example.com" }]
                        }
                    }
                }),
                Some("https://ai.google.dev".to_string()),
            ),
        );
        manager.providers.insert(
            "api-y".to_string(),
            Provider::with_id(
                "api-y".to_string(),
                "API Provider".to_string(),
                json!({
                    "env": {
                        "GEMINI_API_KEY": "sk-api-y",
                        "GOOGLE_GEMINI_BASE_URL": "https://api.example.com",
                        "GEMINI_MODEL": "gemini-2.5-pro"
                    }
                }),
                Some("https://api.example.com".to_string()),
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");

    // Seed Gemini live files from imported profile, then switch away to trigger backfill.
    ProviderService::switch(&state, AppType::Gemini, "imported-x")
        .expect("switch to imported provider should succeed");
    ProviderService::switch(&state, AppType::Gemini, "api-y")
        .expect("switch to api provider should succeed");

    let providers = state
        .db
        .get_all_providers(AppType::Gemini.as_str())
        .expect("read providers");
    assert_eq!(providers.len(), 2, "switch should not remove providers");

    let imported = providers
        .get("imported-x")
        .expect("imported provider should still exist");

    assert_eq!(
        imported
            .settings_config
            .pointer("/env/GOOGLE_CLOUD_PROJECT")
            .and_then(|v| v.as_str()),
        Some("project-x"),
        "backfill should keep full Gemini env (GOOGLE_CLOUD_PROJECT)"
    );
    assert_eq!(
        imported
            .settings_config
            .pointer("/env/GOOGLE_CLOUD_LOCATION")
            .and_then(|v| v.as_str()),
        Some("us-central1"),
        "backfill should keep non-whitelisted Gemini env fields"
    );
    assert_eq!(
        imported
            .settings_config
            .pointer("/config/customSetting")
            .and_then(|v| v.as_str()),
        Some("keep-me"),
        "backfill should preserve existing Gemini config snapshot"
    );
    assert_eq!(
        imported
            .settings_config
            .pointer("/authFiles/enabled")
            .and_then(|v| v.as_bool()),
        Some(true),
        "backfill should preserve authFiles management settings"
    );
    assert_eq!(
        imported
            .settings_config
            .pointer("/authFiles/googleAccounts/accounts/0/email")
            .and_then(|v| v.as_str()),
        Some("imported@example.com"),
        "backfill should preserve authFiles payload"
    );
}

#[test]
fn switch_gemini_with_auth_files_enabled_writes_both_files() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "oauth-profile".to_string();
        manager.providers.insert(
            "oauth-profile".to_string(),
            Provider::with_id(
                "oauth-profile".to_string(),
                "Google OAuth".to_string(),
                json!({
                    "env": {},
                    "authFiles": {
                        "enabled": true,
                        "googleAccounts": { "accounts": [{ "email": "a@b.com" }] },
                        "oauthCreds": { "refresh_token": "rt-1" }
                    }
                }),
                Some("https://ai.google.dev".to_string()),
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");
    ProviderService::switch(&state, AppType::Gemini, "oauth-profile")
        .expect("switch should succeed");

    let google_accounts: serde_json::Value =
        read_json_file(&home.join(".gemini").join("google_accounts.json"))
            .expect("read google_accounts");
    let oauth_creds: serde_json::Value =
        read_json_file(&home.join(".gemini").join("oauth_creds.json")).expect("read oauth_creds");

    assert_eq!(
        google_accounts
            .pointer("/accounts/0/email")
            .and_then(|v| v.as_str()),
        Some("a@b.com")
    );
    assert_eq!(
        oauth_creds
            .get("refresh_token")
            .and_then(|v| v.as_str()),
        Some("rt-1")
    );
}

#[test]
fn switch_gemini_with_auth_files_disabled_preserves_existing_files() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    std::fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    std::fs::write(
        gemini_dir.join("google_accounts.json"),
        serde_json::to_string_pretty(&json!({ "accounts": [{ "email": "keep@x.com" }] }))
            .expect("serialize google_accounts"),
    )
    .expect("seed google_accounts");
    std::fs::write(
        gemini_dir.join("oauth_creds.json"),
        serde_json::to_string_pretty(&json!({ "refresh_token": "keep-rt" }))
            .expect("serialize oauth_creds"),
    )
    .expect("seed oauth_creds");

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "api-profile".to_string();
        manager.providers.insert(
            "api-profile".to_string(),
            Provider::with_id(
                "api-profile".to_string(),
                "API Key Profile".to_string(),
                json!({
                    "env": {
                        "GEMINI_API_KEY": "k1",
                        "GOOGLE_GEMINI_BASE_URL": "https://www.packyapi.com"
                    },
                    "authFiles": {
                        "enabled": false,
                        "googleAccounts": { "accounts": [{ "email": "new@x.com" }] },
                        "oauthCreds": { "refresh_token": "new-rt" }
                    }
                }),
                Some("https://www.packyapi.com".to_string()),
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");
    ProviderService::switch(&state, AppType::Gemini, "api-profile")
        .expect("switch should succeed");

    let google_accounts: serde_json::Value =
        read_json_file(&home.join(".gemini").join("google_accounts.json"))
            .expect("read google_accounts");
    let oauth_creds: serde_json::Value =
        read_json_file(&home.join(".gemini").join("oauth_creds.json")).expect("read oauth_creds");

    assert_eq!(
        google_accounts
            .pointer("/accounts/0/email")
            .and_then(|v| v.as_str()),
        Some("keep@x.com")
    );
    assert_eq!(
        oauth_creds
            .get("refresh_token")
            .and_then(|v| v.as_str()),
        Some("keep-rt")
    );
}

#[test]
fn switch_gemini_with_auth_files_null_deletes_target_file() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    std::fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    std::fs::write(
        gemini_dir.join("google_accounts.json"),
        serde_json::to_string_pretty(&json!({ "accounts": [{ "email": "old@x.com" }] }))
            .expect("serialize google_accounts"),
    )
    .expect("seed google_accounts");

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "oauth-profile".to_string();
        manager.providers.insert(
            "oauth-profile".to_string(),
            Provider::with_id(
                "oauth-profile".to_string(),
                "Google OAuth".to_string(),
                json!({
                    "env": {},
                    "authFiles": {
                        "enabled": true,
                        "googleAccounts": null
                    }
                }),
                Some("https://ai.google.dev".to_string()),
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");
    ProviderService::switch(&state, AppType::Gemini, "oauth-profile")
        .expect("switch should succeed");

    assert!(
        !home.join(".gemini").join("google_accounts.json").exists(),
        "google_accounts.json should be removed when value is null"
    );
}

#[test]
fn switch_gemini_with_auth_files_partial_only_updates_specified_file() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    std::fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    std::fs::write(
        gemini_dir.join("google_accounts.json"),
        serde_json::to_string_pretty(&json!({ "accounts": [{ "email": "old@x.com" }] }))
            .expect("serialize google_accounts"),
    )
    .expect("seed google_accounts");
    std::fs::write(
        gemini_dir.join("oauth_creds.json"),
        serde_json::to_string_pretty(&json!({ "refresh_token": "keep-rt" }))
            .expect("serialize oauth_creds"),
    )
    .expect("seed oauth_creds");

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "oauth-profile".to_string();
        manager.providers.insert(
            "oauth-profile".to_string(),
            Provider::with_id(
                "oauth-profile".to_string(),
                "Google OAuth".to_string(),
                json!({
                    "env": {},
                    "authFiles": {
                        "enabled": true,
                        "googleAccounts": { "accounts": [{ "email": "new@x.com" }] }
                    }
                }),
                Some("https://ai.google.dev".to_string()),
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");
    ProviderService::switch(&state, AppType::Gemini, "oauth-profile")
        .expect("switch should succeed");

    let google_accounts: serde_json::Value =
        read_json_file(&home.join(".gemini").join("google_accounts.json"))
            .expect("read google_accounts");
    let oauth_creds: serde_json::Value =
        read_json_file(&home.join(".gemini").join("oauth_creds.json")).expect("read oauth_creds");

    assert_eq!(
        google_accounts
            .pointer("/accounts/0/email")
            .and_then(|v| v.as_str()),
        Some("new@x.com")
    );
    assert_eq!(
        oauth_creds
            .get("refresh_token")
            .and_then(|v| v.as_str()),
        Some("keep-rt"),
        "oauth_creds should be preserved when field is absent"
    );
}

#[test]
fn provider_service_switch_claude_updates_live_and_state() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent).expect("create claude settings dir");
    }
    let legacy_live = json!({
        "env": {
            "ANTHROPIC_API_KEY": "legacy-key"
        },
        "workspace": {
            "path": "/tmp/workspace"
        }
    });
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&legacy_live).expect("serialize legacy live"),
    )
    .expect("seed claude live config");

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Claude)
            .expect("claude manager");
        manager.current = "old-provider".to_string();
        manager.providers.insert(
            "old-provider".to_string(),
            Provider::with_id(
                "old-provider".to_string(),
                "Legacy Claude".to_string(),
                json!({
                    "env": { "ANTHROPIC_API_KEY": "stale-key" }
                }),
                None,
            ),
        );
        manager.providers.insert(
            "new-provider".to_string(),
            Provider::with_id(
                "new-provider".to_string(),
                "Fresh Claude".to_string(),
                json!({
                    "env": { "ANTHROPIC_API_KEY": "fresh-key" },
                    "workspace": { "path": "/tmp/new-workspace" }
                }),
                None,
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::switch(&state, AppType::Claude, "new-provider")
        .expect("switch provider should succeed");

    let live_after: serde_json::Value =
        read_json_file(&settings_path).expect("read claude live settings");
    assert_eq!(
        live_after
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .and_then(|key| key.as_str()),
        Some("fresh-key"),
        "live settings.json should reflect new provider auth"
    );

    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    let current_id = state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get current provider");
    assert_eq!(
        current_id.as_deref(),
        Some("new-provider"),
        "current provider updated"
    );

    let legacy_provider = providers
        .get("old-provider")
        .expect("legacy provider still exists");
    // With partial merge backfill, only key fields are extracted from live config
    assert_eq!(
        legacy_provider
            .settings_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .and_then(|key| key.as_str()),
        Some("legacy-key"),
        "previous provider should receive backfilled auth key"
    );
    assert!(
        legacy_provider.settings_config.get("workspace").is_none(),
        "backfill should NOT include non-key fields like workspace"
    );
}

#[test]
fn provider_service_switch_missing_provider_returns_error() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    let err = ProviderService::switch(&state, AppType::Claude, "missing")
        .expect_err("switching missing provider should fail");
    match err {
        AppError::Message(msg) => {
            assert!(
                msg.contains("不存在") || msg.contains("not found"),
                "expected provider not found message, got {msg}"
            );
        }
        other => panic!("expected Message error for provider not found, got {other:?}"),
    }
}

#[test]
fn provider_service_switch_codex_missing_auth_returns_error() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Codex)
            .expect("codex manager");
        manager.providers.insert(
            "invalid".to_string(),
            Provider::with_id(
                "invalid".to_string(),
                "Broken Codex".to_string(),
                json!({
                    "config": "[mcp_servers.test]\ncommand = \"noop\""
                }),
                None,
            ),
        );
    }

    let state = create_test_state_with_config(&config).expect("create test state");

    let err = ProviderService::switch(&state, AppType::Codex, "invalid")
        .expect_err("switching should fail without auth");
    match err {
        AppError::Config(msg) => assert!(
            msg.contains("auth"),
            "expected auth related message, got {msg}"
        ),
        other => panic!("expected config error, got {other:?}"),
    }
}

#[test]
fn provider_service_delete_codex_removes_provider_and_files() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Codex)
            .expect("codex manager");
        manager.current = "keep".to_string();
        manager.providers.insert(
            "keep".to_string(),
            Provider::with_id(
                "keep".to_string(),
                "Keep".to_string(),
                json!({
                    "auth": {"OPENAI_API_KEY": "keep-key"},
                    "config": ""
                }),
                None,
            ),
        );
        manager.providers.insert(
            "to-delete".to_string(),
            Provider::with_id(
                "to-delete".to_string(),
                "DeleteCodex".to_string(),
                json!({
                    "auth": {"OPENAI_API_KEY": "delete-key"},
                    "config": ""
                }),
                None,
            ),
        );
    }

    let sanitized = sanitize_provider_name("DeleteCodex");
    let codex_dir = home.join(".codex");
    std::fs::create_dir_all(&codex_dir).expect("create codex dir");
    let auth_path = codex_dir.join(format!("auth-{sanitized}.json"));
    let cfg_path = codex_dir.join(format!("config-{sanitized}.toml"));
    std::fs::write(&auth_path, "{}").expect("seed auth file");
    std::fs::write(&cfg_path, "base_url = \"https://example\"").expect("seed config file");

    let app_state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::delete(&app_state, AppType::Codex, "to-delete")
        .expect("delete provider should succeed");

    let providers = app_state
        .db
        .get_all_providers(AppType::Codex.as_str())
        .expect("get all providers");
    assert!(
        !providers.contains_key("to-delete"),
        "provider entry should be removed"
    );
    // v3.7.0+ 不再使用供应商特定文件（如 auth-*.json, config-*.toml）
    // 删除供应商只影响数据库记录，不清理这些旧格式文件
}

#[test]
fn provider_service_delete_claude_removes_provider_files() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Claude)
            .expect("claude manager");
        manager.current = "keep".to_string();
        manager.providers.insert(
            "keep".to_string(),
            Provider::with_id(
                "keep".to_string(),
                "Keep".to_string(),
                json!({
                    "env": { "ANTHROPIC_API_KEY": "keep-key" }
                }),
                None,
            ),
        );
        manager.providers.insert(
            "delete".to_string(),
            Provider::with_id(
                "delete".to_string(),
                "DeleteClaude".to_string(),
                json!({
                    "env": { "ANTHROPIC_API_KEY": "delete-key" }
                }),
                None,
            ),
        );
    }

    let sanitized = sanitize_provider_name("DeleteClaude");
    let claude_dir = home.join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("create claude dir");
    let by_name = claude_dir.join(format!("settings-{sanitized}.json"));
    let by_id = claude_dir.join("settings-delete.json");
    std::fs::write(&by_name, "{}").expect("seed settings by name");
    std::fs::write(&by_id, "{}").expect("seed settings by id");

    let app_state = create_test_state_with_config(&config).expect("create test state");

    ProviderService::delete(&app_state, AppType::Claude, "delete").expect("delete claude provider");

    let providers = app_state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    assert!(
        !providers.contains_key("delete"),
        "claude provider should be removed"
    );
    // v3.7.0+ 不再使用供应商特定文件（如 settings-*.json）
    // 删除供应商只影响数据库记录，不清理这些旧格式文件
}

#[test]
fn provider_service_delete_current_provider_returns_error() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Claude)
            .expect("claude manager");
        manager.current = "keep".to_string();
        manager.providers.insert(
            "keep".to_string(),
            Provider::with_id(
                "keep".to_string(),
                "Keep".to_string(),
                json!({
                    "env": { "ANTHROPIC_API_KEY": "keep-key" }
                }),
                None,
            ),
        );
    }

    let app_state = create_test_state_with_config(&config).expect("create test state");

    let err = ProviderService::delete(&app_state, AppType::Claude, "keep")
        .expect_err("deleting current provider should fail");
    match err {
        AppError::Localized { zh, .. } => assert!(
            zh.contains("不能删除当前正在使用的供应商")
                || zh.contains("无法删除当前正在使用的供应商"),
            "unexpected message: {zh}"
        ),
        AppError::Config(msg) => assert!(
            msg.contains("不能删除当前正在使用的供应商")
                || msg.contains("无法删除当前正在使用的供应商"),
            "unexpected message: {msg}"
        ),
        AppError::Message(msg) => assert!(
            msg.contains("不能删除当前正在使用的供应商")
                || msg.contains("无法删除当前正在使用的供应商"),
            "unexpected message: {msg}"
        ),
        other => panic!("expected Config/Message error, got {other:?}"),
    }
}

fn create_basic_provider(app_type: AppType, id: &str) -> Provider {
    match app_type {
        AppType::Claude => Provider::with_id(
            id.to_string(),
            "Claude Provider".to_string(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "claude-key",
                    "ANTHROPIC_BASE_URL": "https://api.anthropic.com"
                }
            }),
            None,
        ),
        AppType::Codex => Provider::with_id(
            id.to_string(),
            "Codex Provider".to_string(),
            json!({
                "auth": {
                    "OPENAI_API_KEY": "codex-key"
                },
                "config": r#"model_provider = "openai"
model = "gpt-4o-mini"
[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
"#
            }),
            None,
        ),
        AppType::Gemini => Provider::with_id(
            id.to_string(),
            "Gemini Provider".to_string(),
            json!({
                "env": {
                    "GEMINI_API_KEY": "gemini-key",
                    "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com"
                },
                "config": {}
            }),
            None,
        ),
        AppType::OpenCode => Provider::with_id(
            id.to_string(),
            "OpenCode Provider".to_string(),
            json!({
                "npm": "@openrouter/ai-sdk-provider",
                "options": {
                    "apiKey": "open-code-key",
                    "baseURL": "https://openrouter.ai/api/v1"
                },
                "models": {
                    "default": {
                        "name": "openai/gpt-4o-mini"
                    }
                }
            }),
            None,
        ),
        AppType::OpenClaw => Provider::with_id(
            id.to_string(),
            "OpenClaw Provider".to_string(),
            json!({
                "baseUrl": "https://api.openai.com/v1",
                "apiKey": "openclaw-key",
                "api": "openai",
                "models": [
                    {
                        "id": "gpt-4o-mini"
                    }
                ]
            }),
            None,
        ),
    }
}

fn create_config_with_current(app_type: AppType, provider_id: &str) -> MultiAppConfig {
    let mut config = MultiAppConfig::default();
    let manager = config
        .get_manager_mut(&app_type)
        .expect("app manager should exist");
    manager.current = provider_id.to_string();
    manager.providers.insert(
        provider_id.to_string(),
        create_basic_provider(app_type, provider_id),
    );
    config
}

fn seed_live_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(path, content).expect("seed live file");
}

#[test]
fn logout_context_claude_deletes_live_files_and_clears_current() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    seed_live_file(
        &home.join(".claude").join("settings.json"),
        r#"{"env":{"ANTHROPIC_API_KEY":"x"}}"#,
    );
    seed_live_file(
        &home.join(".claude").join("claude.json"),
        r#"{"env":{"ANTHROPIC_API_KEY":"x"}}"#,
    );
    seed_live_file(&home.join(".claude.json"), r#"{"mcpServers":{}}"#);

    let config = create_config_with_current(AppType::Claude, "claude-current");
    let state = create_test_state_with_config(&config).expect("create test state");
    state
        .db
        .set_current_provider("claude", "claude-current")
        .expect("set current provider");
    assert_eq!(
        ProviderService::current(&state, AppType::Claude).expect("current"),
        "claude-current"
    );

    ProviderService::logout_context(&state, AppType::Claude).expect("logout should succeed");

    assert!(
        !home.join(".claude").join("settings.json").exists(),
        "settings.json should be deleted"
    );
    assert!(
        !home.join(".claude").join("claude.json").exists(),
        "claude.json should be deleted"
    );
    assert!(
        !home.join(".claude.json").exists(),
        "~/.claude.json should be deleted"
    );
    assert_eq!(
        state
            .db
            .get_current_provider("claude")
            .expect("db current provider"),
        None
    );
    assert_eq!(
        ProviderService::current(&state, AppType::Claude).expect("effective current"),
        ""
    );
}

#[test]
fn logout_context_codex_deletes_live_files_and_clears_current() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    seed_live_file(
        &home.join(".codex").join("auth.json"),
        r#"{"OPENAI_API_KEY":"x"}"#,
    );
    seed_live_file(
        &home.join(".codex").join("config.toml"),
        r#"model_provider = "openai""#,
    );

    let config = create_config_with_current(AppType::Codex, "codex-current");
    let state = create_test_state_with_config(&config).expect("create test state");
    state
        .db
        .set_current_provider("codex", "codex-current")
        .expect("set current provider");
    assert_eq!(
        ProviderService::current(&state, AppType::Codex).expect("current"),
        "codex-current"
    );

    ProviderService::logout_context(&state, AppType::Codex).expect("logout should succeed");

    assert!(
        !home.join(".codex").join("auth.json").exists(),
        "auth.json should be deleted"
    );
    assert!(
        !home.join(".codex").join("config.toml").exists(),
        "config.toml should be deleted"
    );
    assert_eq!(
        state
            .db
            .get_current_provider("codex")
            .expect("db current provider"),
        None
    );
    assert_eq!(
        ProviderService::current(&state, AppType::Codex).expect("effective current"),
        ""
    );
}

#[test]
fn logout_context_gemini_deletes_live_files_and_clears_current() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    seed_live_file(
        &home.join(".gemini").join(".env"),
        "GEMINI_API_KEY=gemini-key\n",
    );
    seed_live_file(&home.join(".gemini").join("settings.json"), r#"{"security":{}}"#);
    seed_live_file(
        &home.join(".gemini").join("google_accounts.json"),
        r#"{"accounts":[]}"#,
    );
    seed_live_file(
        &home.join(".gemini").join("oauth_creds.json"),
        r#"{"refresh_token":"x"}"#,
    );

    let config = create_config_with_current(AppType::Gemini, "gemini-current");
    let state = create_test_state_with_config(&config).expect("create test state");
    state
        .db
        .set_current_provider("gemini", "gemini-current")
        .expect("set current provider");
    assert_eq!(
        ProviderService::current(&state, AppType::Gemini).expect("current"),
        "gemini-current"
    );

    ProviderService::logout_context(&state, AppType::Gemini).expect("logout should succeed");

    assert!(
        !home.join(".gemini").join(".env").exists(),
        ".env should be deleted"
    );
    assert!(
        !home.join(".gemini").join("settings.json").exists(),
        "settings.json should be deleted"
    );
    assert!(
        !home.join(".gemini").join("google_accounts.json").exists(),
        "google_accounts.json should be deleted"
    );
    assert!(
        !home.join(".gemini").join("oauth_creds.json").exists(),
        "oauth_creds.json should be deleted"
    );
    assert_eq!(
        state
            .db
            .get_current_provider("gemini")
            .expect("db current provider"),
        None
    );
    assert_eq!(
        ProviderService::current(&state, AppType::Gemini).expect("effective current"),
        ""
    );
}

#[test]
fn logout_context_opencode_deletes_live_files_and_clears_current_flags() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let mut settings = AppSettings::default();
    settings.opencode_config_dir = Some(
        home.join(".config")
            .join("opencode")
            .to_string_lossy()
            .to_string(),
    );
    update_settings(settings).expect("set opencode test override");

    seed_live_file(
        &home.join(".config").join("opencode").join("opencode.json"),
        r#"{"provider":{}}"#,
    );
    seed_live_file(
        &home.join(".config").join("opencode").join(".env"),
        "OPENCODE_API_KEY=key\n",
    );

    let config = create_config_with_current(AppType::OpenCode, "opencode-current");
    let state = create_test_state_with_config(&config).expect("create test state");
    state
        .db
        .set_current_provider("opencode", "opencode-current")
        .expect("set current provider");
    assert_eq!(
        state
            .db
            .get_current_provider("opencode")
            .expect("db current provider")
            .as_deref(),
        Some("opencode-current")
    );

    ProviderService::logout_context(&state, AppType::OpenCode).expect("logout should succeed");

    assert!(
        !home.join(".config").join("opencode").join("opencode.json").exists(),
        "opencode.json should be deleted"
    );
    assert!(
        !home.join(".config").join("opencode").join(".env").exists(),
        ".env should be deleted"
    );
    assert_eq!(
        state
            .db
            .get_current_provider("opencode")
            .expect("db current provider"),
        None
    );
}

#[test]
fn logout_context_openclaw_deletes_live_files_and_clears_current_flags() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let mut settings = AppSettings::default();
    settings.openclaw_config_dir = Some(
        home.join(".openclaw")
            .to_string_lossy()
            .to_string(),
    );
    update_settings(settings).expect("set openclaw test override");

    seed_live_file(
        &home.join(".openclaw").join("openclaw.json"),
        r#"{"providers":{}}"#,
    );

    let config = create_config_with_current(AppType::OpenClaw, "openclaw-current");
    let state = create_test_state_with_config(&config).expect("create test state");
    state
        .db
        .set_current_provider("openclaw", "openclaw-current")
        .expect("set current provider");
    assert_eq!(
        state
            .db
            .get_current_provider("openclaw")
            .expect("db current provider")
            .as_deref(),
        Some("openclaw-current")
    );

    ProviderService::logout_context(&state, AppType::OpenClaw).expect("logout should succeed");

    assert!(
        !home.join(".openclaw").join("openclaw.json").exists(),
        "openclaw.json should be deleted"
    );
    assert_eq!(
        state
            .db
            .get_current_provider("openclaw")
            .expect("db current provider"),
        None
    );
}

#[test]
fn logout_context_missing_files_succeeds_idempotently() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let config = create_config_with_current(AppType::Gemini, "gemini-current");
    let state = create_test_state_with_config(&config).expect("create test state");
    state
        .db
        .set_current_provider("gemini", "gemini-current")
        .expect("set current provider");

    let result =
        ProviderService::logout_context(&state, AppType::Gemini).expect("logout should succeed");

    assert!(result, "logout should return true");
    assert_eq!(
        state
            .db
            .get_current_provider("gemini")
            .expect("db current provider"),
        None
    );
}

#[test]
fn logout_context_blocks_when_proxy_takeover_active_for_switch_mode_apps() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let config = create_config_with_current(AppType::Claude, "claude-current");
    let state = create_test_state_with_config(&config).expect("create test state");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime");

    runtime
        .block_on(state.db.save_live_backup(
            "claude",
            r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:15721"}}"#,
        ))
        .expect("seed live backup");
    runtime
        .block_on(state.proxy_service.start())
        .expect("start proxy");

    let result = ProviderService::logout_context(&state, AppType::Claude);

    let _ = runtime.block_on(state.proxy_service.stop());

    let err = result.expect_err("logout should be blocked during takeover");
    let err_message = err.to_string();
    assert!(
        err_message.contains("proxy takeover mode")
            || err_message.contains("代理接管模式")
            || err_message.contains("takeover"),
        "unexpected error message: {err_message}"
    );
}
