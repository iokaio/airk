// SPDX-License-Identifier: Apache-2.0
//! Where agent configuration lives: project files (agentMigration.ts
//! `locations`) and user-level files (globalAgentInventory.ts
//! `globalConfigLocations`). Pure: the platform and environment are inputs.
//! Location never grants ownership; entries still need recognition.
use super::Agent;

/// Project-relative configuration files, in the TS `locations` order.
pub(crate) const PROJECT_FILES: [(Agent, &[&str]); 8] = [
    (Agent::Codex, &[".codex", "config.toml"]),
    (Agent::Claude, &[".mcp.json"]),
    (Agent::Copilot, &[".vscode", "mcp.json"]),
    (Agent::Gemini, &[".gemini", "settings.json"]),
    (Agent::Antigravity, &[".agents", "mcp_config.json"]),
    (Agent::Cursor, &[".cursor", "mcp.json"]),
    (Agent::Roo, &[".roo", "mcp.json"]),
    (Agent::Junie, &[".junie", "mcp", "mcp.json"]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    Windows,
    Macos,
    Linux,
}

impl Platform {
    pub(crate) fn host() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        }
    }

    fn separator(self) -> char {
        if self == Platform::Windows { '\\' } else { '/' }
    }

    fn absolute(self, value: &str) -> bool {
        let bytes = value.as_bytes();
        match self {
            Platform::Windows => {
                value.starts_with("\\\\")
                    || (bytes.len() >= 3
                        && bytes[0].is_ascii_alphabetic()
                        && bytes[1] == b':'
                        && matches!(bytes[2], b'\\' | b'/'))
            }
            _ => value.starts_with('/'),
        }
    }

    fn join(self, base: &str, parts: &[&str]) -> String {
        let separator = self.separator();
        let mut out = base.trim_end_matches(['/', '\\']).to_string();
        out.push(separator);
        out.push_str(&parts.join(&separator.to_string()));
        out
    }

    fn same(self, left: &str, right: &str) -> bool {
        if self == Platform::Windows {
            left.eq_ignore_ascii_case(right)
        } else {
            left == right
        }
    }
}

/// The environment the user-level locations depend on.
#[derive(Clone, Debug, Default)]
pub(crate) struct UserEnv {
    pub(crate) home: Option<String>,
    pub(crate) appdata: Option<String>,
    pub(crate) codex_home: Option<String>,
    pub(crate) claude_config_dir: Option<String>,
    pub(crate) xdg_config_home: Option<String>,
    pub(crate) vscode_portable: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UserLocations {
    pub(crate) files: Vec<(Agent, String)>,
    /// VS Code `User` folders, whose `profiles` may add files.
    pub(crate) vscode_roots: Vec<String>,
    pub(crate) blockers: Vec<String>,
}

impl UserLocations {
    fn add(&mut self, platform: Platform, agent: Agent, file: String) {
        if !self
            .files
            .iter()
            .any(|(known, path)| *known == agent && platform.same(path, &file))
        {
            self.files.push((agent, file));
        }
    }
}

fn checked(
    platform: Platform,
    value: &Option<String>,
    label: &str,
    blockers: &mut Vec<String>,
) -> Option<String> {
    let value = value.as_deref()?;
    if value.is_empty()
        || value.len() > 4096
        || value.chars().any(char::is_control)
        || !platform.absolute(value)
    {
        blockers.push(format!("invalid_{label}"));
        return None;
    }
    Some(value.to_string())
}

/// User-level Codex, Claude Code and VS Code files.
pub(crate) fn user_locations(platform: Platform, env: &UserEnv) -> UserLocations {
    let mut out = UserLocations::default();
    let Some(home) = checked(platform, &env.home, "user_home", &mut out.blockers) else {
        return out;
    };
    out.add(
        platform,
        Agent::Codex,
        platform.join(&home, &[".codex", "config.toml"]),
    );
    if let Some(codex) = checked(platform, &env.codex_home, "codex_home", &mut out.blockers) {
        out.add(
            platform,
            Agent::Codex,
            platform.join(&codex, &["config.toml"]),
        );
    }
    out.add(
        platform,
        Agent::Claude,
        platform.join(&home, &[".claude.json"]),
    );
    if let Some(claude) = checked(
        platform,
        &env.claude_config_dir,
        "claude_config_dir",
        &mut out.blockers,
    ) {
        out.add(
            platform,
            Agent::Claude,
            platform.join(&claude, &[".claude.json"]),
        );
    }
    let base = match platform {
        Platform::Windows => checked(platform, &env.appdata, "appdata", &mut out.blockers)
            .unwrap_or_else(|| platform.join(&home, &["AppData", "Roaming"])),
        Platform::Macos => platform.join(&home, &["Library", "Application Support"]),
        Platform::Linux => checked(
            platform,
            &env.xdg_config_home,
            "xdg_config_home",
            &mut out.blockers,
        )
        .unwrap_or_else(|| platform.join(&home, &[".config"])),
    };
    let mut roots: Vec<String> = ["Code", "Code - Insiders"]
        .into_iter()
        .map(|product| platform.join(&base, &[product, "User"]))
        .collect();
    if let Some(portable) = checked(
        platform,
        &env.vscode_portable,
        "vscode_portable",
        &mut out.blockers,
    ) {
        roots.push(platform.join(&portable, &["user-data", "User"]));
    }
    for root in roots {
        if !out.vscode_roots.contains(&root) {
            out.add(
                platform,
                Agent::Copilot,
                platform.join(&root, &["mcp.json"]),
            );
            out.vscode_roots.push(root);
        }
    }
    out
}
