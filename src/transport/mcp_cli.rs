//! `tark mcp` CLI helpers (file + manager operations).
//!
//! These functions are intentionally decoupled from storage internals:
//! they operate on [`crate::mcp::client::McpServerManager`] plus explicit
//! TOML file paths using only the `toml` crate. Persistence stays
//! file-based (`mcp/servers.toml` shape: top-level `[servers.<id>]` tables)
//! so a future `main.rs` wiring can pass either the global or project file.
//!
//! Env-key mapping for Streamable HTTP (see `mcp::types::resolve_endpoint`):
//! - `MCP_URL` carries the http(s) endpoint (absent => stdio).
//! - `MCP_BEARER_ENV` names the env var holding the bearer token.
//! - `MCP_ALLOW_INSECURE=1` opts into plain http to non-loopback hosts.
//! - `MCP_HEADER_<NAME>` entries become extra headers.

#![allow(dead_code)]

use crate::mcp::client::{conformance_check, McpServerManager};
use crate::mcp::types::{ConformanceReport, McpInspectSummary};
use crate::storage::McpServer;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One row for `mcp list` output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpListEntry {
    /// Server id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// In-memory enabled flag.
    pub enabled: bool,
    /// Whether currently connected.
    pub connected: bool,
    /// Status display string.
    pub status: String,
    /// Transport label (`stdio` / `streamable-http`).
    pub transport: String,
    /// Number of discovered tools.
    pub tools: usize,
}

/// Read a `servers.toml` file into a sorted map of server id -> TOML value.
///
/// Missing files yield an empty map. The expected shape is
/// `{ servers: { <id>: { name, command, args, env, ... } } }`.
pub fn read_servers_file(path: &Path) -> Result<BTreeMap<String, toml::Value>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    if content.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let root: toml::Value =
        toml::from_str(&content).with_context(|| format!("Failed to parse {}", path.display()))?;
    let mut out = BTreeMap::new();
    if let Some(servers) = root.get("servers").and_then(|v| v.as_table()) {
        for (id, entry) in servers {
            out.insert(id.clone(), entry.clone());
        }
    }
    Ok(out)
}

/// Render servers as canonical TOML with sorted keys.
///
/// Keys are emitted in lexicographic order (via [`BTreeMap`]) so output is
/// deterministic across runs. Pure function (no I/O) for unit tests.
pub fn canonical_toml_string(servers: &BTreeMap<String, toml::Value>) -> Result<String> {
    let mut servers_table = toml::map::Map::new();
    // BTreeMap iteration is already sorted; insert in order.
    for (id, entry) in servers {
        let value = entry.clone();
        // Ensure each entry is a table.
        if !value.is_table() {
            anyhow::bail!("Server '{}' entry must be a TOML table", id);
        }
        servers_table.insert(id.clone(), value);
    }
    let mut root = toml::map::Map::new();
    root.insert("servers".to_string(), toml::Value::Table(servers_table));
    let doc = toml::Value::Table(root);
    toml::to_string_pretty(&doc).context("Failed to serialize servers TOML")
}

/// Write servers to a file using the canonical sorted-keys writer.
pub fn write_servers_file(path: &Path, servers: &BTreeMap<String, toml::Value>) -> Result<()> {
    let content = canonical_toml_string(servers)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
    }
    // Rollback safety (R12): keep one backup generation and write atomically
    // (temp file + rename) so a crash cannot corrupt the servers file.
    if path.exists() {
        let backup = path.with_extension("toml.bak");
        std::fs::copy(path, &backup)
            .with_context(|| format!("Failed to back up {}", path.display()))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, content).with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

/// Convert a TOML server entry into a [`McpServer`].
pub fn toml_entry_to_server(entry: &toml::Value) -> Result<McpServer> {
    entry
        .clone()
        .try_into()
        .context("Invalid server entry shape")
}

/// Convert a [`McpServer`] into a TOML value.
pub fn server_to_toml_entry(server: &McpServer) -> Result<toml::Value> {
    toml::Value::try_from(server).context("Failed to encode server entry")
}

/// Parse JSON import into sorted server entries.
///
/// Accepts `{ "mcpServers": { ... } }` (Claude-style) or
/// `{ "servers": { ... } }` (tark shape). Each server value may contain:
/// `command` (string), `args` (array), `env` (object), `url` (string),
/// `name` (string), `enabled` (bool), `capabilities` (array).
/// `url` is mapped to `env.MCP_URL` (see module docs).
pub fn parse_import_json(json_str: &str) -> Result<BTreeMap<String, toml::Value>> {
    let root: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse import JSON")?;
    let servers_obj = root
        .get("mcpServers")
        .or_else(|| root.get("servers"))
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            anyhow::anyhow!("Import JSON must contain 'mcpServers' or 'servers' object")
        })?;
    let mut out = BTreeMap::new();
    for (id, value) in servers_obj {
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string();
        let command = value
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let args: Vec<String> = value
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let mut env: BTreeMap<String, String> = value
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(url) = value.get("url").and_then(|v| v.as_str()) {
            if !url.trim().is_empty() {
                env.insert("MCP_URL".to_string(), url.trim().to_string());
            }
        }
        let enabled = value
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let capabilities: Vec<String> = value
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if command.trim().is_empty() && !env.contains_key("MCP_URL") {
            anyhow::bail!(
                "Server '{}' needs 'command' or 'url' (url maps to env.MCP_URL)",
                id
            );
        }
        let mut table = toml::map::Map::new();
        table.insert("name".to_string(), toml::Value::String(name));
        table.insert("command".to_string(), toml::Value::String(command));
        table.insert(
            "args".to_string(),
            toml::Value::Array(args.into_iter().map(toml::Value::String).collect()),
        );
        let mut env_table = toml::map::Map::new();
        for (k, v) in env {
            env_table.insert(k, toml::Value::String(v));
        }
        table.insert("env".to_string(), toml::Value::Table(env_table));
        table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
        table.insert(
            "capabilities".to_string(),
            toml::Value::Array(capabilities.into_iter().map(toml::Value::String).collect()),
        );
        out.insert(id.clone(), toml::Value::Table(table));
    }
    Ok(out)
}

/// List all servers known to the manager.
pub async fn mcp_list(manager: &McpServerManager) -> Vec<McpListEntry> {
    let mut entries = Vec::new();
    for id in manager.server_ids().await {
        let config = manager.get_config(&id).await;
        let status = manager.status(&id).await;
        let tools = manager.tools(&id).await;
        let connected = status.as_ref().map(|s| s.is_connected()).unwrap_or(false);
        let transport = match config.as_ref() {
            Some(cfg) => crate::mcp::types::resolve_endpoint(cfg)
                .transport_label()
                .to_string(),
            None => "unknown".to_string(),
        };
        entries.push(McpListEntry {
            id: id.clone(),
            name: config.map(|c| c.name).unwrap_or_else(|| id.clone()),
            enabled: manager
                .get_config(&id)
                .await
                .map(|c| c.enabled)
                .unwrap_or(false),
            connected,
            status: status
                .map(|s| s.display().to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
            transport,
            tools: tools.len(),
        });
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

/// Add a server: writes the TOML file and registers in memory.
///
/// Refuses silent overwrite: errors when `id` exists unless `force` is true.
pub async fn mcp_add(
    manager: &McpServerManager,
    file_path: &Path,
    id: &str,
    server: McpServer,
    force: bool,
) -> Result<()> {
    let mut servers = read_servers_file(file_path)?;
    if servers.contains_key(id) && !force {
        anyhow::bail!(
            "Server '{}' already exists in {} (use force to overwrite)",
            id,
            file_path.display()
        );
    }
    servers.insert(id.to_string(), server_to_toml_entry(&server)?);
    write_servers_file(file_path, &servers)?;
    manager.register_server(id, server).await;
    Ok(())
}

/// Remove a server from the file and the manager.
pub async fn mcp_remove(manager: &McpServerManager, file_path: &Path, id: &str) -> Result<()> {
    let mut servers = read_servers_file(file_path)?;
    if servers.remove(id).is_none() {
        // Fall back to manager-only removal when the file has no entry
        // (e.g. global vs project split); still error if unknown everywhere.
        if manager.get_config(id).await.is_none() {
            anyhow::bail!("Unknown server: {}", id);
        }
    } else {
        write_servers_file(file_path, &servers)?;
    }
    let _ = manager.disconnect(id).await;
    // Manager removal errors when unknown; ignore when file had the entry
    // but manager never loaded it.
    let _ = manager.remove_server(id).await;
    Ok(())
}

/// Enable a server (file + in-memory; persistence is the TOML file itself).
pub async fn mcp_enable(manager: &McpServerManager, file_path: &Path, id: &str) -> Result<()> {
    let mut servers = read_servers_file(file_path)?;
    if let Some(entry) = servers.get_mut(id) {
        let mut server = toml_entry_to_server(entry)?;
        server.enabled = true;
        *entry = server_to_toml_entry(&server)?;
        write_servers_file(file_path, &servers)?;
        manager.register_server(id, server).await;
    } else {
        // Manager-only when the file does not carry this id.
        manager.set_enabled(id, true).await?;
    }
    Ok(())
}

/// Disable a server (file + in-memory).
pub async fn mcp_disable(manager: &McpServerManager, file_path: &Path, id: &str) -> Result<()> {
    let mut servers = read_servers_file(file_path)?;
    if let Some(entry) = servers.get_mut(id) {
        let mut server = toml_entry_to_server(entry)?;
        server.enabled = false;
        *entry = server_to_toml_entry(&server)?;
        write_servers_file(file_path, &servers)?;
        let _ = manager.disconnect(id).await;
        manager.register_server(id, server).await;
    } else {
        let _ = manager.disconnect(id).await;
        manager.set_enabled(id, false).await?;
    }
    Ok(())
}

/// Inspect a server via the manager.
pub async fn mcp_inspect(manager: &McpServerManager, id: &str) -> Result<McpInspectSummary> {
    manager.inspect(id).await
}

/// Show the informed-trust launch summary for a stdio server (R3 S7).
pub async fn mcp_trust_info(manager: &McpServerManager, id: &str) -> Result<String> {
    manager.trust_summary(id).await
}

/// Record explicit user trust for the current launch configuration (R3 S7).
pub async fn mcp_approve(manager: &McpServerManager, id: &str) -> Result<String> {
    let summary = manager.approve_server(id).await?;
    Ok(format!(
        "Trusted '{}' for this exact launch configuration:\n\n{}",
        id, summary
    ))
}

/// Revoke previously granted trust.
pub async fn mcp_revoke_trust(manager: &McpServerManager, id: &str) -> Result<String> {
    if manager.revoke_trust(id).await? {
        Ok(format!("Revoked trust for '{}'", id))
    } else {
        Ok(format!("No trust record for '{}'", id))
    }
}

/// Connect to a server via the manager.
pub async fn mcp_connect(manager: &McpServerManager, id: &str) -> Result<()> {
    manager.connect(id).await
}

/// Disconnect from a server via the manager.
pub async fn mcp_disconnect(manager: &McpServerManager, id: &str) -> Result<()> {
    manager.disconnect(id).await
}

/// Reconnect to a server via the manager.
pub async fn mcp_reconnect(manager: &McpServerManager, id: &str) -> Result<()> {
    manager.reconnect(id).await
}

/// Run conformance check via the manager.
pub async fn mcp_conformance(manager: &McpServerManager, id: &str) -> ConformanceReport {
    conformance_check(manager, id).await
}

/// Import servers from JSON (`{mcpServers:{...}}` or `{servers:{...}}`).
///
/// Returns imported ids. Refuses silent overwrite per id unless `force`.
pub async fn mcp_import(
    manager: &McpServerManager,
    file_path: &Path,
    json_str: &str,
    force: bool,
) -> Result<Vec<String>> {
    let imported = parse_import_json(json_str)?;
    let mut servers = read_servers_file(file_path)?;
    let mut ids = Vec::new();
    for (id, entry) in imported {
        if servers.contains_key(&id) && !force {
            anyhow::bail!(
                "Server '{}' already exists in {} (use force to overwrite)",
                id,
                file_path.display()
            );
        }
        let server = toml_entry_to_server(&entry)?;
        servers.insert(id.clone(), entry);
        manager.register_server(&id, server).await;
        ids.push(id);
    }
    ids.sort();
    write_servers_file(file_path, &servers)?;
    Ok(ids)
}

/// Dispatch `tark mcp <action>`-style calls.
///
/// `action` is one of: list, inspect, trust, approve, revoke-trust, enable,
/// disable, connect, disconnect, reconnect, remove, conformance. Mutating
/// file actions (enable/disable/remove) use `file_path`; `target` carries
/// the server id when required.
/// Returns human-readable output. `add`/`import` need structured payloads;
/// use [`mcp_add`] / [`mcp_import`] directly for those.
pub async fn run_mcp_cli(
    manager: &McpServerManager,
    action: &str,
    target: Option<&str>,
    file_path: &Path,
) -> Result<String> {
    match action {
        "list" => {
            let entries = mcp_list(manager).await;
            Ok(serde_json::to_string_pretty(&entries)?)
        }
        "inspect" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("inspect needs a server id"))?;
            Ok(serde_json::to_string_pretty(&mcp_inspect(manager, id).await?)?)
        }
        "trust" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("trust needs a server id"))?;
            Ok(mcp_trust_info(manager, id).await?)
        }
        "approve" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("approve needs a server id"))?;
            Ok(mcp_approve(manager, id).await?)
        }
        "revoke-trust" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("revoke-trust needs a server id"))?;
            Ok(mcp_revoke_trust(manager, id).await?)
        }
        "enable" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("enable needs a server id"))?;
            mcp_enable(manager, file_path, id).await?;
            Ok(format!("Enabled '{}'", id))
        }
        "disable" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("disable needs a server id"))?;
            mcp_disable(manager, file_path, id).await?;
            Ok(format!("Disabled '{}'", id))
        }
        "connect" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("connect needs a server id"))?;
            mcp_connect(manager, id).await?;
            Ok(format!("Connected '{}'", id))
        }
        "disconnect" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("disconnect needs a server id"))?;
            mcp_disconnect(manager, id).await?;
            Ok(format!("Disconnected '{}'", id))
        }
        "reconnect" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("reconnect needs a server id"))?;
            mcp_reconnect(manager, id).await?;
            Ok(format!("Reconnected '{}'", id))
        }
        "remove" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("remove needs a server id"))?;
            mcp_remove(manager, file_path, id).await?;
            Ok(format!("Removed '{}'", id))
        }
        "conformance" => {
            let id = target.ok_or_else(|| anyhow::anyhow!("conformance needs a server id"))?;
            Ok(serde_json::to_string_pretty(
                &mcp_conformance(manager, id).await,
            )?)
        }
        other => anyhow::bail!(
            "Unknown mcp action '{}': expected list/add/inspect/trust/approve/revoke-trust/enable/disable/connect/disconnect/reconnect/remove/import/conformance",
            other
        ),
    }
}

/// Default file path helper: `<dir>/servers.toml`.
pub fn servers_file_in(dir: &Path) -> PathBuf {
    dir.join("servers.toml")
}

/// Entry point for `tark mcp <action> [target] [--scope project|global] [--cwd DIR]`.
///
/// Resolves the scope to a servers file, loads it into a fresh manager, and
/// dispatches via [`run_mcp_cli`]. Returns human-readable output for stdout.
/// Mutating actions that need structured payloads (`add`, `import`) are not
/// supported through this argv form; manage those via the project file or a
/// future `--json` flag.
pub async fn run_mcp_command(
    action: &str,
    target: Option<&str>,
    scope: &str,
    cwd: Option<&str>,
) -> Result<String> {
    if matches!(action, "add" | "import") {
        anyhow::bail!(
            "'tark mcp {}' needs a structured payload; edit {} directly or use the TUI",
            action,
            match scope {
                "global" => "~/.config/tark/mcp/servers.toml",
                _ => ".tark/mcp/servers.toml",
            }
        );
    }
    let working_dir = cwd
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let file_path = match scope {
        "global" => {
            let global = crate::storage::GlobalStorage::new()?;
            global.root().join("mcp").join("servers.toml")
        }
        "project" | "" => {
            let storage = crate::storage::TarkStorage::new(&working_dir)?;
            storage.project_root().join("mcp").join("servers.toml")
        }
        other => anyhow::bail!("Unknown scope '{}': expected project|global", other),
    };

    let manager = McpServerManager::new(
        working_dir.join(".tark").join("mcp-data"),
        working_dir.clone(),
    );
    // Load file entries into the manager (missing file => empty set).
    for (id, entry) in read_servers_file(&file_path)? {
        if let Ok(server) = toml_entry_to_server(&entry) {
            manager.register_server(&id, server).await;
        } else {
            tracing::warn!("Skipping invalid MCP server entry '{}'", id);
        }
    }

    run_mcp_cli(&manager, action, target, &file_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_server(name: &str, command: &str) -> McpServer {
        McpServer {
            name: name.to_string(),
            command: command.to_string(),
            args: vec![],
            env: HashMap::new(),
            enabled: true,
            capabilities: vec![],
            tark: None,
        }
    }

    #[test]
    fn canonical_writer_orders_keys() {
        let mut servers = BTreeMap::new();
        for id in ["zebra", "apple", "mango"] {
            servers.insert(
                id.to_string(),
                server_to_toml_entry(&sample_server(id, "npx")).unwrap(),
            );
        }
        let text = canonical_toml_string(&servers).unwrap();
        let za = text.find("zebra").unwrap();
        let aa = text.find("apple").unwrap();
        let ma = text.find("mango").unwrap();
        assert!(aa < ma && ma < za, "keys must be sorted: {}", text);
    }

    #[test]
    fn import_accepts_both_shapes_and_maps_url() {
        let claude = r#"{"mcpServers":{"gh":{"command":"npx","args":["-y","x"],"env":{"A":"1"}}}}"#;
        let parsed = parse_import_json(claude).unwrap();
        assert!(parsed.contains_key("gh"));

        let tark_shape = r#"{"servers":{"remote":{"url":"https://example.com/mcp","env":{}}}}"#;
        let parsed2 = parse_import_json(tark_shape).unwrap();
        let server = toml_entry_to_server(&parsed2["remote"]).unwrap();
        assert_eq!(
            server.env.get("MCP_URL").map(String::as_str),
            Some("https://example.com/mcp")
        );

        assert!(parse_import_json(r#"{"other":{}}"#).is_err());
        assert!(parse_import_json(r#"{"servers":{"bad":{"env":{}}}}"#).is_err());
    }

    #[test]
    fn read_missing_file_is_empty() {
        let missing = std::env::temp_dir().join("tark-mcp-missing-xyz-123.toml");
        let _ = std::fs::remove_file(&missing);
        assert!(read_servers_file(&missing).unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_refuses_overwrite_without_force() {
        let dir = std::env::temp_dir().join(format!("tark-mcp-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("servers.toml");
        let _ = std::fs::remove_file(&file);
        let manager = McpServerManager::new(std::env::temp_dir(), std::env::temp_dir());
        mcp_add(&manager, &file, "demo", sample_server("Demo", "npx"), false)
            .await
            .unwrap();
        let err = mcp_add(&manager, &file, "demo", sample_server("Demo", "npx"), false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        mcp_add(&manager, &file, "demo", sample_server("Demo", "node"), true)
            .await
            .unwrap();
        let _ = std::fs::remove_file(&file);
    }
}
