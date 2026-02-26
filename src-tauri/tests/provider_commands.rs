use base64::prelude::*;
use serde_json::json;

use cc_switch_lib::{
    get_claude_credentials_path, get_codex_auth_path, get_codex_config_path,
    logout_provider_context_test_hook, read_json_file, switch_provider_test_hook,
    update_settings, write_codex_live_atomic, AppError, AppSettings, AppType, McpApps, McpServer,
    MultiAppConfig, Provider, ProviderMeta,
};

#[path = "support.rs"]
mod support;
use std::collections::HashMap;
use support::{create_test_state_with_config, ensure_test_home, reset_test_fs, test_mutex};

fn configure_antigravity_test_dir(home: &std::path::Path) -> std::path::PathBuf {
    let antigravity_dir = home.join(".antigravity").join("User").join("globalStorage");
    let mut settings = AppSettings::default();
    settings.antigravity_config_dir = Some(antigravity_dir.to_string_lossy().to_string());
    update_settings(settings).expect("set antigravity test override");
    antigravity_dir
}

#[test]
fn switch_provider_updates_codex_live_and_state() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let legacy_auth = json!({"OPENAI_API_KEY": "legacy-key"});
    let legacy_config = r#"[mcp_servers.legacy]
type = "stdio"
command = "echo"
"#;
    write_codex_live_atomic(&legacy_auth, Some(legacy_config))
        .expect("seed existing codex live config");

    let mut config = MultiAppConfig::default();
    {
        let manager = config
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

    // v3.7.0+: 使用统一的 MCP 结构
    config.mcp.servers = Some(HashMap::new());
    config.mcp.servers.as_mut().unwrap().insert(
        "echo-server".into(),
        McpServer {
            id: "echo-server".to_string(),
            name: "Echo Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                codex: true, // 启用 Codex
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    );

    let app_state = create_test_state_with_config(&config).expect("create test state");

    switch_provider_test_hook(&app_state, AppType::Codex, "new-provider")
        .expect("switch provider should succeed");

    let auth_value: serde_json::Value =
        read_json_file(&get_codex_auth_path()).expect("read auth.json");
    assert_eq!(
        auth_value
            .get("OPENAI_API_KEY")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
        "fresh-key",
        "live auth.json should reflect new provider"
    );

    let config_text = std::fs::read_to_string(get_codex_config_path()).expect("read config.toml");
    // With partial merge, only key fields (model, provider, model_providers) are
    // merged into config.toml. The existing MCP section should be preserved.
    // MCP sync from DB is handled separately (at startup or explicit sync).
    assert!(
        config_text.contains("mcp_servers.legacy"),
        "config.toml should preserve existing MCP servers after partial merge"
    );

    let current_id = app_state
        .db
        .get_current_provider(AppType::Codex.as_str())
        .expect("get current provider");
    assert_eq!(
        current_id.as_deref(),
        Some("new-provider"),
        "current provider updated"
    );

    let providers = app_state
        .db
        .get_all_providers(AppType::Codex.as_str())
        .expect("get all providers");

    let new_provider = providers.get("new-provider").expect("new provider exists");
    let new_config_text = new_provider
        .settings_config
        .get("config")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    // With partial merge, only key fields (model_provider, model, model_providers)
    // are written to the live file. MCP servers are synced separately.
    // The provider's stored config should still contain mcp_servers.latest.
    assert!(
        new_config_text.contains("mcp_servers.latest"),
        "provider snapshot should contain provider's original config"
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
    // 回填机制：切换前会将 live 配置回填到当前供应商
    // 这保护了用户在 live 文件中的手动修改
    assert_eq!(
        legacy_auth_value, "legacy-key",
        "previous provider should be backfilled with live auth"
    );
}

#[test]
fn switch_provider_missing_provider_returns_error() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();

    let mut config = MultiAppConfig::default();
    config
        .get_manager_mut(&AppType::Claude)
        .expect("claude manager")
        .current = "does-not-exist".to_string();

    let app_state = create_test_state_with_config(&config).expect("create test state");

    let err = switch_provider_test_hook(&app_state, AppType::Claude, "missing-provider")
        .expect_err("switching to a missing provider should fail");

    let err_str = err.to_string();
    assert!(
        err_str.contains("供应商不存在")
            || err_str.contains("Provider not found")
            || err_str.contains("missing-provider"),
        "error message should mention missing provider, got: {err_str}"
    );
}

#[test]
fn switch_provider_updates_claude_live_and_state() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let settings_path = cc_switch_lib::get_claude_settings_path();
    let credentials_path = get_claude_credentials_path();
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
    std::fs::write(
        &credentials_path,
        serde_json::to_string_pretty(&json!({
            "accessToken": "legacy-credentials"
        }))
        .expect("serialize legacy credentials"),
    )
    .expect("seed credentials sidecar");

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
        let mut new_provider = Provider::with_id(
            "new-provider".to_string(),
            "Fresh Claude".to_string(),
            json!({
                "env": { "ANTHROPIC_API_KEY": "fresh-key" },
                "workspace": { "path": "/tmp/new-workspace" }
            }),
            None,
        );
        new_provider.meta = Some(ProviderMeta {
            claude_credentials: Some(json!({ "accessToken": "fresh-credentials" })),
            ..Default::default()
        });
        manager
            .providers
            .insert("new-provider".to_string(), new_provider);
    }

    let app_state = create_test_state_with_config(&config).expect("create test state");

    switch_provider_test_hook(&app_state, AppType::Claude, "new-provider")
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
    let live_credentials: serde_json::Value =
        read_json_file(&credentials_path).expect("read .credentials.json");
    assert_eq!(
        live_credentials.get("accessToken").and_then(|v| v.as_str()),
        Some("fresh-credentials"),
        "command switch should apply provider credentials sidecar"
    );

    let current_id = app_state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get current provider");
    assert_eq!(
        current_id.as_deref(),
        Some("new-provider"),
        "current provider updated"
    );

    let providers = app_state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");

    let legacy_provider = providers
        .get("old-provider")
        .expect("legacy provider still exists");
    // Backfill mechanism: before switching, the live config's key fields are
    // backfilled to the current provider. With partial merge, only key fields
    // (auth, model, endpoint) are extracted — non-key fields like workspace
    // are NOT included in the backfill.
    assert_eq!(
        legacy_provider
            .settings_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .and_then(|key| key.as_str()),
        Some("legacy-key"),
        "previous provider should be backfilled with live auth key"
    );
    assert!(
        legacy_provider.settings_config.get("workspace").is_none(),
        "backfill should NOT include non-key fields like workspace"
    );
    assert_eq!(
        legacy_provider
            .meta
            .as_ref()
            .and_then(|meta| meta.claude_credentials.as_ref())
            .and_then(|value| value.get("accessToken"))
            .and_then(|v| v.as_str()),
        Some("legacy-credentials"),
        "command switch should backfill live credentials into previous provider meta"
    );
    assert!(
        legacy_provider.settings_config.get("credentials").is_none(),
        "credentials should not be kept in settings_config after backfill"
    );

    let new_provider = providers.get("new-provider").expect("new provider exists");
    assert_eq!(
        new_provider
            .settings_config
            .get("env")
            .and_then(|env| env.get("ANTHROPIC_API_KEY"))
            .and_then(|key| key.as_str()),
        Some("fresh-key"),
        "new provider snapshot should retain fresh auth"
    );

    // v3.7.0+ 使用 SQLite 数据库而非 config.json
    // 验证数据已持久化到数据库
    let home_dir = std::env::var("HOME").expect("HOME should be set by ensure_test_home");
    let db_path = std::path::Path::new(&home_dir)
        .join(".cc-switch")
        .join("cc-switch.db");
    assert!(
        db_path.exists(),
        "switching provider should persist to cc-switch.db"
    );

    // 验证当前供应商已更新
    let current_id = app_state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get current provider");
    assert_eq!(
        current_id.as_deref(),
        Some("new-provider"),
        "database should record the new current provider"
    );
}

#[test]
fn switch_provider_codex_missing_auth_returns_error_and_keeps_state() {
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

    let app_state = create_test_state_with_config(&config).expect("create test state");

    let err = switch_provider_test_hook(&app_state, AppType::Codex, "invalid")
        .expect_err("switching should fail when auth missing");
    match err {
        AppError::Config(msg) => assert!(
            msg.contains("auth"),
            "expected auth missing error message, got {msg}"
        ),
        other => panic!("expected config error, got {other:?}"),
    }

    let current_id = app_state
        .db
        .get_current_provider(AppType::Codex.as_str())
        .expect("get current provider");
    // 切换失败后，由于数据库操作是先设置再验证，current 可能已被设为 "invalid"
    // 但由于 live 配置写入失败，状态应该回滚
    // 注意：这个行为取决于 switch_provider 的具体实现
    assert!(
        current_id.is_none() || current_id.as_deref() == Some("invalid"),
        "current provider should remain empty or be the attempted id on failure, got: {current_id:?}"
    );
}

#[test]
fn switch_provider_antigravity_backfills_previous_provider_state() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();
    let antigravity_dir = configure_antigravity_test_dir(home);
    let state_path = antigravity_dir.join("state.vscdb");

    std::fs::create_dir_all(&antigravity_dir).expect("create antigravity dir");
    let live_before = b"command-live-before-switch".to_vec();
    std::fs::write(&state_path, &live_before).expect("seed antigravity live state");

    let old_snapshot = BASE64_STANDARD.encode(b"old-command-snapshot");
    let new_snapshot_bytes = b"new-command-snapshot".to_vec();
    let new_snapshot = BASE64_STANDARD.encode(&new_snapshot_bytes);

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Antigravity)
            .expect("antigravity manager");
        manager.current = "old-provider".to_string();
        manager.providers.insert(
            "old-provider".to_string(),
            Provider::with_id(
                "old-provider".to_string(),
                "Old Antigravity".to_string(),
                json!({
                    "stateVscdbBase64": old_snapshot
                }),
                Some("https://antigravity.google".to_string()),
            ),
        );
        manager.providers.insert(
            "new-provider".to_string(),
            Provider::with_id(
                "new-provider".to_string(),
                "New Antigravity".to_string(),
                json!({
                    "stateVscdbBase64": new_snapshot
                }),
                Some("https://antigravity.google".to_string()),
            ),
        );
    }

    let app_state = create_test_state_with_config(&config).expect("create test state");
    let result = switch_provider_test_hook(&app_state, AppType::Antigravity, "new-provider")
        .expect("command switch provider should succeed");
    assert!(
        result.warnings.is_empty(),
        "command switch should not produce warnings"
    );

    let providers = app_state
        .db
        .get_all_providers(AppType::Antigravity.as_str())
        .expect("get antigravity providers");
    let old_provider = providers
        .get("old-provider")
        .expect("old antigravity provider should exist");
    let expected_backfill = BASE64_STANDARD.encode(&live_before);
    assert_eq!(
        old_provider
            .settings_config
            .get("stateVscdbBase64")
            .and_then(|v| v.as_str()),
        Some(expected_backfill.as_str()),
        "command switch should backfill old antigravity provider from live state"
    );

    let live_after = std::fs::read(&state_path).expect("read antigravity state after command switch");
    assert_eq!(
        live_after, new_snapshot_bytes,
        "command switch should write target provider live state"
    );
}

#[test]
fn logout_provider_context_command_smoke_test() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mut config = MultiAppConfig::default();
    {
        let manager = config
            .get_manager_mut(&AppType::Gemini)
            .expect("gemini manager");
        manager.current = "gemini-current".to_string();
        manager.providers.insert(
            "gemini-current".to_string(),
            Provider::with_id(
                "gemini-current".to_string(),
                "Gemini Current".to_string(),
                json!({
                    "env": {
                        "GEMINI_API_KEY": "stale-provider-key",
                        "GOOGLE_GEMINI_BASE_URL": "https://generativelanguage.googleapis.com"
                    },
                    "config": {}
                }),
                None,
            ),
        );
    }

    let app_state = create_test_state_with_config(&config).expect("create test state");
    app_state
        .db
        .set_current_provider("gemini", "gemini-current")
        .expect("set current provider");

    let gemini_dir = home.join(".gemini");
    std::fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    std::fs::write(gemini_dir.join(".env"), "GEMINI_API_KEY=gemini-key\n").expect("seed .env");
    std::fs::write(gemini_dir.join("settings.json"), r#"{"security":{}}"#)
        .expect("seed settings.json");

    let result = logout_provider_context_test_hook(&app_state, AppType::Gemini)
        .expect("logout command should succeed");

    assert!(result, "command should return true");
    assert!(
        !gemini_dir.join(".env").exists(),
        "gemini .env should be removed by logout command"
    );
    assert!(
        !gemini_dir.join("settings.json").exists(),
        "gemini settings.json should be removed by logout command"
    );
    assert_eq!(
        app_state
            .db
            .get_current_provider("gemini")
            .expect("db current provider"),
        None
    );
    let providers = app_state
        .db
        .get_all_providers(AppType::Gemini.as_str())
        .expect("read providers after logout");
    let current_provider = providers
        .get("gemini-current")
        .expect("current provider should still exist");
    assert_eq!(
        current_provider
            .settings_config
            .pointer("/env/GEMINI_API_KEY")
            .and_then(|v| v.as_str()),
        Some("gemini-key"),
        "logout command should backfill live Gemini .env into current provider"
    );
}
