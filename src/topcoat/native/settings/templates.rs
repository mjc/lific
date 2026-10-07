//! Pinned client setup choices from Main's connected-tools flow.

#[derive(Clone, Copy)]
pub(super) struct OsPaths {
    pub(super) linux: Option<&'static str>,
    pub(super) mac: Option<&'static str>,
    pub(super) windows: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub(super) enum SetupStep {
    Text(&'static str),
    Command(&'static str),
}

#[derive(Clone, Copy)]
pub(super) struct ToolTemplate {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) paths: OsPaths,
    pub(super) notes: &'static [SetupStep],
    pub(super) config: &'static str,
    pub(super) env_var: Option<&'static str>,
}

pub(super) const MCP_URL_MARKER: &str = "__LIFIC_MCP_URL__";
pub(super) const API_KEY_MARKER: &str = "__LIFIC_API_KEY__";

const fn shared_paths(linux: &'static str, windows: &'static str) -> OsPaths {
    OsPaths {
        linux: Some(linux),
        mac: Some(linux),
        windows: Some(windows),
    }
}

pub(super) const GENERIC_TEMPLATE: ToolTemplate = ToolTemplate {
    id: "custom",
    name: "Custom MCP connection",
    description: "Custom MCP connection",
    paths: OsPaths {
        linux: None,
        mac: None,
        windows: None,
    },
    notes: &[SetupStep::Text(
        "Use this URL and Authorization header in your client's HTTP MCP settings. The exact config format depends on your client.",
    )],
    config: r#"{
  "url": "__LIFIC_MCP_URL__",
  "headers": { "Authorization": "Bearer __LIFIC_API_KEY__" }
}"#,
    env_var: None,
};

pub(super) const TOOL_TEMPLATES: [ToolTemplate; 8] = [
    ToolTemplate {
        id: "opencode",
        name: "OpenCode",
        description: "Anomaly's open-source agentic coding CLI",
        paths: shared_paths(
            "~/.config/opencode/opencode.json",
            "%USERPROFILE%\\.config\\opencode\\opencode.json",
        ),
        notes: &[SetupStep::Text(
            "Add this to the \"mcp\" section of your config.",
        )],
        config: r#"{
  "lific": {
    "type": "remote",
    "url": "__LIFIC_MCP_URL__",
    "headers": { "Authorization": "Bearer __LIFIC_API_KEY__" }
  }
}"#,
        env_var: None,
    },
    ToolTemplate {
        id: "cursor",
        name: "Cursor",
        description: "AI-first code editor by Anysphere",
        paths: shared_paths(
            "~/.cursor/mcp.json (global) · .cursor/mcp.json (project)",
            "%USERPROFILE%\\.cursor\\mcp.json (global) · .cursor\\mcp.json (project)",
        ),
        notes: &[SetupStep::Text(
            "Add this to the \"mcpServers\" section, then reload Cursor.",
        )],
        config: r#"{
  "lific": {
    "url": "__LIFIC_MCP_URL__",
    "headers": { "Authorization": "Bearer __LIFIC_API_KEY__" }
  }
}"#,
        env_var: None,
    },
    ToolTemplate {
        id: "claude-code",
        name: "Claude Code",
        description: "Anthropic's CLI coding agent",
        paths: shared_paths(
            "~/.claude.json (user scope)",
            "%USERPROFILE%\\.claude.json (user scope)",
        ),
        notes: &[
            SetupStep::Text("Easiest: run this command (it writes the config for you):"),
            SetupStep::Command(
                "claude mcp add --transport http --scope user lific __LIFIC_MCP_URL__ --header \"Authorization: Bearer <key>\"",
            ),
            SetupStep::Text("Or add the block below to the \"mcpServers\" section manually."),
        ],
        config: r#"{
  "lific": {
    "type": "http",
    "url": "__LIFIC_MCP_URL__",
    "headers": { "Authorization": "Bearer __LIFIC_API_KEY__" }
  }
}"#,
        env_var: None,
    },
    ToolTemplate {
        id: "claude",
        name: "Claude Desktop",
        description: "Anthropic's desktop client for Claude (macOS & Windows)",
        paths: OsPaths {
            linux: None,
            mac: Some("~/Library/Application Support/Claude/claude_desktop_config.json"),
            windows: Some("%APPDATA%\\Claude\\claude_desktop_config.json"),
        },
        notes: &[
            SetupStep::Text("Requires mcp-remote (installed automatically by npx)."),
            SetupStep::Text(
                "Add the block below to the \"mcpServers\" section, then fully restart Claude Desktop.",
            ),
        ],
        config: r#"{
  "lific": {
    "command": "npx",
    "args": ["-y", "mcp-remote", "__LIFIC_MCP_URL__"],
    "env": { "AUTHORIZATION": "Bearer __LIFIC_API_KEY__" }
  }
}"#,
        env_var: None,
    },
    ToolTemplate {
        id: "codex",
        name: "Codex",
        description: "OpenAI's CLI coding agent",
        paths: shared_paths("~/.codex/config.toml", "%USERPROFILE%\\.codex\\config.toml"),
        notes: &[
            SetupStep::Text("Add the block below under [mcp_servers] in config.toml."),
            SetupStep::Text("The key is read from the LIFIC_API_KEY env var (set it in step 3)."),
        ],
        config: "[mcp_servers.lific]\ntransport.type = \"http\"\ntransport.url = \"__LIFIC_MCP_URL__\"\ntransport.bearer_token_env_var = \"LIFIC_API_KEY\"",
        env_var: Some("LIFIC_API_KEY"),
    },
    ToolTemplate {
        id: "pi",
        name: "Pi",
        description: "Pi coding agent (via pi-mcp-adapter)",
        paths: shared_paths(
            "~/.pi/agent/mcp.json",
            "%USERPROFILE%\\.pi\\agent\\mcp.json",
        ),
        notes: &[
            SetupStep::Text("First install the adapter, then restart Pi:"),
            SetupStep::Command("pi install npm:pi-mcp-adapter"),
            SetupStep::Text(
                "Add the block below to the \"mcpServers\" section. The key is read from the LIFIC_API_KEY env var (set it in step 3).",
            ),
        ],
        config: r#"{
  "lific": {
    "url": "__LIFIC_MCP_URL__",
    "auth": "bearer",
    "bearerTokenEnv": "LIFIC_API_KEY",
    "lifecycle": "keep-alive"
  }
}"#,
        env_var: Some("LIFIC_API_KEY"),
    },
    ToolTemplate {
        id: "vscode",
        name: "VS Code",
        description: "GitHub Copilot agent mode in VS Code",
        paths: shared_paths(
            "~/.config/Code/User/mcp.json (user) · .vscode/mcp.json (workspace)",
            "%APPDATA%\\Code\\User\\mcp.json (user) · .vscode\\mcp.json (workspace)",
        ),
        notes: &[
            SetupStep::Text(
                "Add this to the \"servers\" section. Or run the command palette action:",
            ),
            SetupStep::Command("MCP: Open User Configuration"),
            SetupStep::Text("VS Code 1.101+ with GitHub Copilot is required."),
        ],
        config: r#"{
  "servers": {
    "lific": {
      "type": "http",
      "url": "__LIFIC_MCP_URL__",
      "headers": { "Authorization": "Bearer __LIFIC_API_KEY__" }
    }
  }
}"#,
        env_var: None,
    },
    ToolTemplate {
        id: "zed",
        name: "Zed",
        description: "High-performance Rust-based editor",
        paths: shared_paths(
            "~/.config/zed/settings.json",
            "%APPDATA%\\Zed\\settings.json",
        ),
        notes: &[SetupStep::Text(
            "Add this to the \"context_servers\" section of your Zed settings (Command Palette: zed: open settings).",
        )],
        config: r#"{
  "context_servers": {
    "lific": {
      "url": "__LIFIC_MCP_URL__",
      "headers": { "Authorization": "Bearer __LIFIC_API_KEY__" }
    }
  }
}"#,
        env_var: None,
    },
];

#[derive(Clone)]
pub(super) struct OsPathGroup {
    pub(super) key: &'static str,
    pub(super) label: String,
    pub(super) oses: Vec<&'static str>,
}

pub(super) fn path_for_os(template: ToolTemplate, os: &str) -> Option<&'static str> {
    match os {
        "linux" => template.paths.linux,
        "mac" => template.paths.mac,
        "windows" => template.paths.windows,
        _ => None,
    }
}

pub(super) fn path_groups(template: ToolTemplate) -> Vec<OsPathGroup> {
    let options = [("linux", "Linux"), ("mac", "macOS"), ("windows", "Windows")];
    let mut groups: Vec<OsPathGroup> = Vec::new();
    for (os, label) in options {
        let Some(path) = path_for_os(template, os) else {
            continue;
        };
        if let Some(group) = groups
            .iter_mut()
            .find(|group| path_for_os(template, group.key) == Some(path))
        {
            group.oses.push(os);
            group.label.push_str(" / ");
            group.label.push_str(label);
        } else {
            groups.push(OsPathGroup {
                key: os,
                label: label.to_owned(),
                oses: vec![os],
            });
        }
    }
    groups
}

pub(super) fn config_template(template: ToolTemplate) -> &'static str {
    template.config
}

pub(super) fn export_template(template: ToolTemplate, os: &str) -> Option<String> {
    let variable = template.env_var?;
    Some(if os == "windows" {
        format!(r#"setx {variable} "{API_KEY_MARKER}""#)
    } else {
        format!(r#"export {variable}="{API_KEY_MARKER}""#)
    })
}

pub(super) fn persistence_hint(os: &str) -> &'static str {
    match os {
        "mac" => {
            "Runs in the current shell. To persist it, add the line to ~/.zshrc (the default macOS shell)."
        }
        "windows" => {
            "setx applies to new terminals, not the current one. Reopen your terminal (or Pi) after running it."
        }
        _ => "Runs in the current shell. To persist it, add the line to ~/.bashrc or ~/.profile.",
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::home_fixture;
    use super::{
        GENERIC_TEMPLATE, SetupStep, TOOL_TEMPLATES, config_template, export_template, path_groups,
        persistence_hint,
    };
    use axum::http::StatusCode;

    fn template_value(template: super::ToolTemplate) -> serde_json::Value {
        serde_json::json!({
            "id": template.id,
            "name": template.name,
            "description": template.description,
            "paths": {
                "linux": template.paths.linux,
                "mac": template.paths.mac,
                "windows": template.paths.windows,
            },
            "notes": template.notes.iter().map(|step| match step {
                SetupStep::Text(text) => serde_json::json!({"text": text}),
                SetupStep::Command(command) => serde_json::json!({"command": command}),
            }).collect::<Vec<_>>(),
            "config_template": config_template(template),
            "env_var": template.env_var,
            "groups": path_groups(template).iter().map(|group| serde_json::json!({
                "key": group.key,
                "label": group.label,
                "oses": group.oses,
            })).collect::<Vec<_>>(),
            "exports": {
                "linux": export_template(template, "linux"),
                "mac": export_template(template, "mac"),
                "windows": export_template(template, "windows"),
            },
        })
    }

    #[test]
    fn tool_setup_descriptors_match_main_clients_and_generic_fallback() {
        let result = home_fixture::evaluate_handler(
            "src/topcoat/native/settings/tool_setup.test.cjs",
            &serde_json::json!({
                "templates": TOOL_TEMPLATES.iter().copied().map(template_value).collect::<Vec<_>>(),
                "generic": template_value(GENERIC_TEMPLATE),
                "mounted_url": "https://example.test/prefix/mcp",
                "persistence_hints": {
                    "linux": persistence_hint("linux"),
                    "mac": persistence_hint("mac"),
                    "windows": persistence_hint("windows"),
                },
            }),
        );
        assert_eq!(result["setup_parity"], true);
    }

    #[tokio::test]
    async fn tool_setup_config_uses_the_trusted_mounted_mcp_path() {
        let fixture = home_fixture::fixture();
        let (status, html) =
            home_fixture::document(&fixture, "/app", "/settings", true, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            html.contains("data-native-tool-setup"),
            "the mounted settings page renders client setup"
        );
        assert!(
            html.contains("/app/mcp"),
            "client configuration keeps the trusted mount prefix in its MCP URL"
        );
    }
}
