use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;

use serde_json::json;

use cc_switch_lib::{
    get_claude_credentials_path, get_claude_mcp_path, get_claude_settings_path,
    import_default_config_test_hook, AppError, AppType, McpApps, McpServer, McpService,
    MultiAppConfig, Provider, ProviderService,
};

#[path = "support.rs"]
mod support;
use support::{create_test_state_with_config, ensure_test_home, reset_test_fs, test_mutex};

#[test]
fn import_default_config_claude_persists_provider() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).expect("create claude settings dir");
    }
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "test-key",
            "ANTHROPIC_BASE_URL": "https://api.test"
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("seed claude settings.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Claude);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Claude)
        .expect("import default config succeeds");

    // 验证内存状态
    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    let current_id = state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get current provider");
    assert_eq!(current_id.as_deref(), Some("default"));
    let default_provider = providers.get("default").expect("default provider");
    assert_eq!(
        default_provider.settings_config, settings,
        "default provider should capture live settings"
    );

    // 验证数据已持久化到数据库（v3.7.0+ 使用 SQLite 而非 config.json）
    let db_path = home.join(".cc-switch").join("cc-switch.db");
    assert!(
        db_path.exists(),
        "importing default config should persist to cc-switch.db"
    );
}

#[test]
fn import_default_config_claude_imports_credentials_sidecar() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).expect("create claude settings dir");
    }
    let settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "test-key",
            "ANTHROPIC_BASE_URL": "https://api.test"
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("seed claude settings.json");

    let credentials_path = get_claude_credentials_path();
    let credentials = json!({
        "claudeAiOauth": {
            "accessToken": "oauth-token",
            "refreshToken": "oauth-refresh"
        }
    });
    fs::write(
        &credentials_path,
        serde_json::to_string_pretty(&credentials).expect("serialize credentials"),
    )
    .expect("seed claude .credentials.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Claude);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Claude)
        .expect("import default config succeeds");

    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    let default_provider = providers.get("default").expect("default provider");

    assert_eq!(
        default_provider
            .meta
            .as_ref()
            .and_then(|meta| meta.claude_credentials.as_ref()),
        Some(&credentials),
        "imported profile should include claude credentials sidecar in meta"
    );
    assert!(
        default_provider
            .settings_config
            .get("credentials")
            .is_none(),
        "claude credentials should not be persisted inside settings_config"
    );
}

#[test]
fn import_default_config_claude_with_invalid_credentials_json_returns_error() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).expect("create claude settings dir");
    }
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "test-key"
            }
        }))
        .expect("serialize settings"),
    )
    .expect("seed claude settings.json");

    let credentials_path = get_claude_credentials_path();
    fs::write(&credentials_path, "{invalid-json").expect("seed invalid credentials");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Claude);
    let state = create_test_state_with_config(&config).expect("create test state");

    let err = import_default_config_test_hook(&state, AppType::Claude)
        .expect_err("invalid credentials json should fail import");
    assert!(
        err.to_string().contains(".credentials.json"),
        "error should mention credentials sidecar path, got: {err}"
    );

    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    assert!(
        providers.is_empty(),
        "failed import should not persist providers"
    );
}

#[test]
fn import_default_config_with_existing_provider_creates_new_profile_and_switches_effective_current()
{
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).expect("create claude settings dir");
    }
    let live_settings = json!({
        "env": {
            "ANTHROPIC_AUTH_TOKEN": "imported-key",
            "ANTHROPIC_BASE_URL": "https://imported.example"
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&live_settings).expect("serialize live settings"),
    )
    .expect("seed claude settings.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Claude);
    let state = create_test_state_with_config(&config).expect("create test state");

    let existing = Provider::with_id(
        "existing".to_string(),
        "Existing Provider".to_string(),
        json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "existing-key",
                "ANTHROPIC_BASE_URL": "https://existing.example"
            }
        }),
        None,
    );
    state
        .db
        .save_provider(AppType::Claude.as_str(), &existing)
        .expect("save existing provider");
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "existing")
        .expect("set existing as db current");

    import_default_config_test_hook(&state, AppType::Claude)
        .expect("import default config succeeds with existing provider");

    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    assert_eq!(
        providers.len(),
        2,
        "should keep existing and add one imported"
    );
    assert!(
        providers.contains_key("existing"),
        "existing provider should be preserved"
    );

    let imported_entry = providers
        .iter()
        .find(|(id, _)| id.as_str() != "existing")
        .expect("imported provider should exist");
    let imported_id = imported_entry.0.clone();
    let imported_provider = imported_entry.1;

    assert!(
        imported_id.starts_with("imported-"),
        "imported provider id should use imported-* format, got: {imported_id}"
    );
    assert!(
        imported_provider.name.starts_with("Imported Config ("),
        "imported provider name should use Imported Config timestamp format, got: {}",
        imported_provider.name
    );
    assert_eq!(
        imported_provider.settings_config, live_settings,
        "imported provider should snapshot live settings"
    );
    assert!(
        imported_provider.created_at.is_some(),
        "imported provider should have created_at populated"
    );

    let db_current = state
        .db
        .get_current_provider(AppType::Claude.as_str())
        .expect("get db current");
    assert_eq!(
        db_current.as_deref(),
        Some(imported_id.as_str()),
        "db current should switch to imported provider"
    );

    // Mutate DB current away from imported provider; effective current should
    // still resolve to imported provider via local settings current_provider_*.
    state
        .db
        .set_current_provider(AppType::Claude.as_str(), "existing")
        .expect("set db current back to existing");
    let effective_current =
        ProviderService::current(&state, AppType::Claude).expect("get effective current");
    assert_eq!(
        effective_current, imported_id,
        "effective current should follow local settings updated by import"
    );

    let db_path = home.join(".cc-switch").join("cc-switch.db");
    assert!(
        db_path.exists(),
        "importing config should persist to cc-switch.db"
    );
}

#[test]
fn import_default_config_repeated_calls_generate_unique_imported_ids() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let settings_path = get_claude_settings_path();
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).expect("create claude settings dir");
    }
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&json!({
            "env": {
                "ANTHROPIC_AUTH_TOKEN": "repeat-key",
                "ANTHROPIC_BASE_URL": "https://repeat.example"
            }
        }))
        .expect("serialize settings"),
    )
    .expect("seed claude settings.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Claude);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Claude).expect("first import should succeed");
    import_default_config_test_hook(&state, AppType::Claude).expect("second import should succeed");
    import_default_config_test_hook(&state, AppType::Claude).expect("third import should succeed");

    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    assert_eq!(
        providers.len(),
        3,
        "three imports should create three providers"
    );
    assert!(
        providers.contains_key("default"),
        "first import should stay default"
    );

    let ids: Vec<String> = providers.keys().cloned().collect();
    let unique_ids: HashSet<String> = ids.iter().cloned().collect();
    assert_eq!(
        ids.len(),
        unique_ids.len(),
        "all imported provider ids should be unique"
    );

    let imported_ids: Vec<&String> = providers
        .keys()
        .filter(|id| id.as_str() != "default")
        .collect();
    assert_eq!(
        imported_ids.len(),
        2,
        "imports after the first should generate imported-* providers"
    );
    assert!(
        imported_ids
            .iter()
            .all(|id| id.as_str().starts_with("imported-")),
        "non-default imported ids should start with imported-*"
    );

    let db_path = home.join(".cc-switch").join("cc-switch.db");
    assert!(db_path.exists(), "imports should persist to cc-switch.db");
}

#[test]
fn import_default_config_without_live_file_returns_error() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let state = create_test_state().expect("create test state");

    let err = import_default_config_test_hook(&state, AppType::Claude)
        .expect_err("missing live file should error");
    match err {
        AppError::Localized { zh, .. } => assert!(
            zh.contains("Claude Code 配置文件不存在"),
            "unexpected error message: {zh}"
        ),
        AppError::Message(msg) => assert!(
            msg.contains("Claude Code 配置文件不存在"),
            "unexpected error message: {msg}"
        ),
        other => panic!("unexpected error variant: {other:?}"),
    }

    // 使用数据库架构，不再检查 config.json
    // 失败的导入不应该向数据库写入任何供应商
    let providers = state
        .db
        .get_all_providers(AppType::Claude.as_str())
        .expect("get all providers");
    assert!(
        providers.is_empty(),
        "failed import should not create any providers in database"
    );
}

#[test]
fn import_default_config_gemini_without_env_file_still_imports() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    let settings_path = gemini_dir.join("settings.json");
    let settings = json!({
        "security": {
            "auth": {
                "selectedType": "oauth-personal"
            }
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings).expect("serialize gemini settings"),
    )
    .expect("seed gemini settings.json");
    let _ = fs::remove_file(gemini_dir.join("google_accounts.json"));
    let _ = fs::remove_file(gemini_dir.join("oauth_creds.json"));

    // Intentionally do NOT create ~/.gemini/.env to simulate OAuth-only setup.

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Gemini);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Gemini)
        .expect("import default gemini config succeeds without .env");

    let providers = state
        .db
        .get_all_providers(AppType::Gemini.as_str())
        .expect("get all gemini providers");
    let default_provider = providers.get("default").expect("default provider");

    assert!(
        default_provider
            .settings_config
            .get("env")
            .and_then(|v| v.as_object())
            .is_some(),
        "imported gemini provider should include env object even when .env is missing"
    );
    assert_eq!(
        default_provider
            .settings_config
            .pointer("/config/security/auth/selectedType")
            .and_then(|v| v.as_str()),
        Some("oauth-personal"),
        "import should preserve settings.json config"
    );
    assert_eq!(
        default_provider.category.as_deref(),
        Some("official"),
        "oauth profile should be classified as official"
    );
}

#[test]
fn import_default_config_gemini_reads_env_with_export_prefix() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    fs::write(
        gemini_dir.join(".env"),
        "export GEMINI_API_KEY=sk-exported\nexport GOOGLE_GEMINI_BASE_URL=https://export.example\nGEMINI_MODEL=gemini-3-pro\n",
    )
    .expect("seed gemini .env");
    fs::write(
        gemini_dir.join("settings.json"),
        serde_json::to_string_pretty(&json!({})).expect("serialize settings"),
    )
    .expect("seed settings.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Gemini);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Gemini)
        .expect("import default gemini config succeeds");

    let providers = state
        .db
        .get_all_providers(AppType::Gemini.as_str())
        .expect("get all gemini providers");
    let default_provider = providers.get("default").expect("default provider");

    assert_eq!(
        default_provider
            .settings_config
            .pointer("/env/GEMINI_API_KEY")
            .and_then(|v| v.as_str()),
        Some("sk-exported")
    );
    assert_eq!(
        default_provider
            .settings_config
            .pointer("/env/GOOGLE_GEMINI_BASE_URL")
            .and_then(|v| v.as_str()),
        Some("https://export.example")
    );
    assert_eq!(
        default_provider
            .settings_config
            .pointer("/env/GEMINI_MODEL")
            .and_then(|v| v.as_str()),
        Some("gemini-3-pro")
    );
}

#[test]
fn import_default_config_gemini_imports_oauth_files_when_present() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    fs::write(
        gemini_dir.join("settings.json"),
        serde_json::to_string_pretty(&json!({})).expect("serialize settings"),
    )
    .expect("seed settings.json");
    fs::write(
        gemini_dir.join("google_accounts.json"),
        serde_json::to_string_pretty(&json!({ "accounts": [{ "id": "a1" }] }))
            .expect("serialize google_accounts"),
    )
    .expect("seed google_accounts.json");
    fs::write(
        gemini_dir.join("oauth_creds.json"),
        serde_json::to_string_pretty(&json!({ "refresh_token": "rt1" }))
            .expect("serialize oauth_creds"),
    )
    .expect("seed oauth_creds.json");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Gemini);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Gemini)
        .expect("import default gemini config succeeds");

    let providers = state
        .db
        .get_all_providers(AppType::Gemini.as_str())
        .expect("get gemini providers");
    let provider = providers.get("default").expect("default provider");

    assert_eq!(
        provider
            .settings_config
            .pointer("/authFiles/enabled")
            .and_then(|v| v.as_bool()),
        Some(true),
        "authFiles should be enabled when oauth files exist"
    );
    assert_eq!(
        provider
            .settings_config
            .pointer("/authFiles/googleAccounts/accounts/0/id")
            .and_then(|v| v.as_str()),
        Some("a1")
    );
    assert_eq!(
        provider
            .settings_config
            .pointer("/authFiles/oauthCreds/refresh_token")
            .and_then(|v| v.as_str()),
        Some("rt1")
    );
    assert_eq!(
        provider.category.as_deref(),
        Some("official"),
        "oauth auth files profile should be classified as official"
    );
}

#[test]
fn import_default_config_gemini_skips_oauth_files_when_missing() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    fs::write(
        gemini_dir.join("settings.json"),
        serde_json::to_string_pretty(&json!({})).expect("serialize settings"),
    )
    .expect("seed settings.json");
    let _ = fs::remove_file(gemini_dir.join("google_accounts.json"));
    let _ = fs::remove_file(gemini_dir.join("oauth_creds.json"));

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Gemini);
    let state = create_test_state_with_config(&config).expect("create test state");

    import_default_config_test_hook(&state, AppType::Gemini)
        .expect("import default gemini config succeeds");

    let providers = state
        .db
        .get_all_providers(AppType::Gemini.as_str())
        .expect("get gemini providers");
    let provider = providers.get("default").expect("default provider");
    assert!(
        provider.settings_config.get("authFiles").is_none(),
        "authFiles should be absent when oauth files are missing"
    );
}

#[test]
fn import_mcp_from_claude_creates_config_and_enables_servers() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    let mcp_path = get_claude_mcp_path();
    let claude_json = json!({
        "mcpServers": {
            "echo": {
                "type": "stdio",
                "command": "echo"
            }
        }
    });
    fs::write(
        &mcp_path,
        serde_json::to_string_pretty(&claude_json).expect("serialize claude mcp"),
    )
    .expect("seed ~/.claude.json");

    let config = MultiAppConfig::default();
    let state = create_test_state_with_config(&config).expect("create test state");

    let changed = McpService::import_from_claude(&state).expect("import mcp from claude succeeds");
    assert!(
        changed > 0,
        "import should report inserted or normalized entries"
    );

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers
        .get("echo")
        .expect("server imported into unified structure");
    assert!(
        entry.apps.claude,
        "imported server should have Claude app enabled"
    );

    // 验证数据已持久化到数据库
    let db_path = home.join(".cc-switch").join("cc-switch.db");
    assert!(
        db_path.exists(),
        "state.save should persist to cc-switch.db when changes detected"
    );
}

#[test]
fn import_mcp_from_claude_invalid_json_preserves_state() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let _home = ensure_test_home();

    let mcp_path = get_claude_mcp_path();
    fs::write(&mcp_path, "{\"mcpServers\":") // 不完整 JSON
        .expect("seed invalid ~/.claude.json");

    let state = create_test_state().expect("create test state");

    let err =
        McpService::import_from_claude(&state).expect_err("invalid json should bubble up error");
    match err {
        AppError::McpValidation(msg) => assert!(
            msg.contains("解析 ~/.claude.json 失败"),
            "unexpected error message: {msg}"
        ),
        other => panic!("unexpected error variant: {other:?}"),
    }

    // 使用数据库架构，检查 MCP 服务器未被写入
    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    assert!(
        servers.is_empty(),
        "failed import should not persist any MCP servers to database"
    );
}

#[test]
fn set_mcp_enabled_for_codex_writes_live_config() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 创建 Codex 配置目录和文件
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    fs::write(
        codex_dir.join("auth.json"),
        r#"{"OPENAI_API_KEY":"test-key"}"#,
    )
    .expect("create auth.json");
    fs::write(codex_dir.join("config.toml"), "").expect("create empty config.toml");

    let mut config = MultiAppConfig::default();
    config.ensure_app(&AppType::Codex);

    // v3.7.0: 使用统一结构
    config.mcp.servers = Some(HashMap::new());
    config.mcp.servers.as_mut().unwrap().insert(
        "codex-server".into(),
        McpServer {
            id: "codex-server".to_string(),
            name: "Codex Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                codex: false, // 初始未启用
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    );

    let state = create_test_state_with_config(&config).expect("create test state");

    // v3.7.0: 使用 toggle_app 替代 set_enabled
    McpService::toggle_app(&state, "codex-server", AppType::Codex, true)
        .expect("toggle_app should succeed");

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers.get("codex-server").expect("codex server exists");
    assert!(
        entry.apps.codex,
        "server should have Codex app enabled after toggle"
    );

    let toml_path = cc_switch_lib::get_codex_config_path();
    assert!(
        toml_path.exists(),
        "enabling server should trigger sync to ~/.codex/config.toml"
    );
    let toml_text = fs::read_to_string(&toml_path).expect("read codex config");
    assert!(
        toml_text.contains("codex-server"),
        "codex config should include the enabled server definition"
    );
}

#[test]
fn enabling_codex_mcp_skips_when_codex_dir_missing() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 确认 Codex 配置目录不存在（模拟“未安装/未运行过 Codex CLI”）
    assert!(
        !home.join(".codex").exists(),
        "~/.codex should not exist in fresh test environment"
    );

    let state = create_test_state().expect("create test state");

    // 先插入一个未启用 Codex 的 MCP 服务器（避免 upsert 触发同步）
    McpService::upsert_server(
        &state,
        McpServer {
            id: "codex-server".to_string(),
            name: "Codex Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                codex: false,
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("insert server without syncing");

    // 启用 Codex：目录缺失时应跳过写入（不创建 ~/.codex/config.toml）
    McpService::toggle_app(&state, "codex-server", AppType::Codex, true)
        .expect("toggle codex should succeed even when ~/.codex is missing");

    assert!(
        !home.join(".codex").exists(),
        "~/.codex should still not exist after skipped sync"
    );
}

#[test]
fn upsert_mcp_server_disabling_app_removes_from_claude_live_config() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 模拟 Claude 已安装/已初始化：存在 ~/.claude 目录
    fs::create_dir_all(home.join(".claude")).expect("create ~/.claude dir");

    // 先创建一个启用 Claude 的 MCP 服务器
    let state = support::create_test_state().expect("create test state");
    McpService::upsert_server(
        &state,
        McpServer {
            id: "echo".to_string(),
            name: "echo".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: true,
                codex: false,
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("upsert should sync to Claude live config");

    // 确认已写入 ~/.claude.json
    let mcp_path = get_claude_mcp_path();
    let text = fs::read_to_string(&mcp_path).expect("read ~/.claude.json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse ~/.claude.json");
    assert!(
        v.pointer("/mcpServers/echo").is_some(),
        "echo should exist in Claude live config after enabling"
    );

    // 再次 upsert：取消勾选 Claude（apps.claude=false），应从 Claude live 配置中移除
    McpService::upsert_server(
        &state,
        McpServer {
            id: "echo".to_string(),
            name: "echo".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                codex: false,
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("upsert disabling app should remove from Claude live config");

    let text = fs::read_to_string(&mcp_path).expect("read ~/.claude.json after disable");
    let v: serde_json::Value = serde_json::from_str(&text).expect("parse ~/.claude.json");
    assert!(
        v.pointer("/mcpServers/echo").is_none(),
        "echo should be removed from Claude live config after disabling"
    );
}

#[test]
fn import_mcp_from_multiple_apps_merges_enabled_flags() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 1) Claude: ~/.claude.json
    let mcp_path = get_claude_mcp_path();
    let claude_json = json!({
        "mcpServers": {
            "shared": {
                "type": "stdio",
                "command": "echo"
            }
        }
    });
    fs::write(
        &mcp_path,
        serde_json::to_string_pretty(&claude_json).expect("serialize claude mcp"),
    )
    .expect("seed ~/.claude.json");

    // 2) Codex: ~/.codex/config.toml
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).expect("create codex dir");
    fs::write(
        codex_dir.join("config.toml"),
        r#"[mcp_servers.shared]
type = "stdio"
command = "echo"
"#,
    )
    .expect("seed ~/.codex/config.toml");

    let state = support::create_test_state().expect("create test state");

    McpService::import_from_claude(&state).expect("import from claude");
    McpService::import_from_codex(&state).expect("import from codex");

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers.get("shared").expect("shared server exists");
    assert!(entry.apps.claude, "shared should enable Claude");
    assert!(entry.apps.codex, "shared should enable Codex");
}

#[test]
fn import_mcp_from_gemini_sse_url_only_is_valid() {
    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // Gemini MCP 位于 ~/.gemini/settings.json
    let gemini_dir = home.join(".gemini");
    fs::create_dir_all(&gemini_dir).expect("create gemini dir");
    let settings_path = gemini_dir.join("settings.json");

    // Gemini SSE：只包含 url（Gemini 不使用 type 字段）
    let gemini_settings = json!({
        "mcpServers": {
            "sse-server": {
                "url": "https://example.com/sse"
            }
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&gemini_settings).expect("serialize gemini settings"),
    )
    .expect("seed ~/.gemini/settings.json");

    let state = support::create_test_state().expect("create test state");
    let changed = McpService::import_from_gemini(&state).expect("import from gemini");
    assert!(changed > 0, "should import at least 1 server");

    let servers = state.db.get_all_mcp_servers().expect("get all mcp servers");
    let entry = servers.get("sse-server").expect("sse-server exists");
    assert!(entry.apps.gemini, "imported server should enable Gemini");
    assert_eq!(
        entry.server.get("type").and_then(|v| v.as_str()),
        Some("sse"),
        "Gemini url-only server should be normalized to type=sse in unified structure"
    );
}

#[test]
fn enabling_gemini_mcp_skips_when_gemini_dir_missing() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 确认 Gemini 配置目录不存在（模拟“未安装/未运行过 Gemini CLI”）
    assert!(
        !home.join(".gemini").exists(),
        "~/.gemini should not exist in fresh test environment"
    );

    let state = create_test_state().expect("create test state");

    // 先插入一个未启用 Gemini 的 MCP 服务器（避免 upsert 触发同步）
    McpService::upsert_server(
        &state,
        McpServer {
            id: "gemini-server".to_string(),
            name: "Gemini Server".to_string(),
            server: json!({
                "type": "sse",
                "url": "https://example.com/sse"
            }),
            apps: McpApps {
                claude: false,
                codex: false,
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("insert server without syncing");

    // 启用 Gemini：目录缺失时应跳过写入（不创建 ~/.gemini/settings.json）
    McpService::toggle_app(&state, "gemini-server", AppType::Gemini, true)
        .expect("toggle gemini should succeed even when ~/.gemini is missing");

    assert!(
        !home.join(".gemini").exists(),
        "~/.gemini should still not exist after skipped sync"
    );
}

#[test]
fn enabling_claude_mcp_skips_when_claude_config_absent() {
    use support::create_test_state;

    let _guard = test_mutex().lock().expect("acquire test mutex");
    reset_test_fs();
    let home = ensure_test_home();

    // 确认 Claude 相关目录/文件都不存在（模拟“未安装/未运行过 Claude”）
    assert!(
        !home.join(".claude").exists(),
        "~/.claude should not exist in fresh test environment"
    );
    assert!(
        !home.join(".claude.json").exists(),
        "~/.claude.json should not exist in fresh test environment"
    );

    let state = create_test_state().expect("create test state");

    // 先插入一个未启用 Claude 的 MCP 服务器（避免 upsert 触发同步）
    McpService::upsert_server(
        &state,
        McpServer {
            id: "claude-server".to_string(),
            name: "Claude Server".to_string(),
            server: json!({
                "type": "stdio",
                "command": "echo"
            }),
            apps: McpApps {
                claude: false,
                codex: false,
                gemini: false,
                opencode: false,
            },
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        },
    )
    .expect("insert server without syncing");

    // 启用 Claude：配置缺失时应跳过写入（不创建 ~/.claude.json）
    McpService::toggle_app(&state, "claude-server", AppType::Claude, true)
        .expect("toggle claude should succeed even when ~/.claude is missing");

    assert!(
        !home.join(".claude.json").exists(),
        "~/.claude.json should still not exist after skipped sync"
    );
}
