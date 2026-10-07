//! Pinned account-page setup choices from Main's connected-tools flow.

#[derive(Clone, Copy)]
pub(super) struct ToolTemplate {
    pub(super) id: &'static str,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
}

pub(super) const TOOL_TEMPLATES: [ToolTemplate; 8] = [
    ToolTemplate {
        id: "opencode",
        name: "OpenCode",
        description: "Anomaly's open-source agentic coding CLI",
    },
    ToolTemplate {
        id: "cursor",
        name: "Cursor",
        description: "AI-first code editor by Anysphere",
    },
    ToolTemplate {
        id: "claude-code",
        name: "Claude Code",
        description: "Anthropic's CLI coding agent",
    },
    ToolTemplate {
        id: "claude",
        name: "Claude Desktop",
        description: "Anthropic's desktop client for Claude (macOS & Windows)",
    },
    ToolTemplate {
        id: "codex",
        name: "Codex",
        description: "OpenAI's CLI coding agent",
    },
    ToolTemplate {
        id: "pi",
        name: "Pi",
        description: "Pi coding agent (via pi-mcp-adapter)",
    },
    ToolTemplate {
        id: "vscode",
        name: "VS Code",
        description: "GitHub Copilot agent mode in VS Code",
    },
    ToolTemplate {
        id: "zed",
        name: "Zed",
        description: "High-performance Rust-based editor",
    },
];
