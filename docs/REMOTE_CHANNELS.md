# Remote Channels

Tark can run channel plugins (Discord/Slack/etc.) in remote mode so you can drive sessions from chat while keeping a local TUI or headless watcher.

## Quick Start (Discord)

1) Create a Discord application (Developer Portal)
- Add a Bot user
- Copy the Bot token
- Copy the Application ID and Public Key

2) Enable Message Content intent (for plain DM messages)
- Discord app → Bot → **Privileged Gateway Intents**
- Turn **Message Content Intent** ON

3) Configure the plugin (encrypted)

```bash
tark plugin auth discord
```

4) Start remote mode (Gateway)

```bash
# TUI + remote control
tark --remote discord

# Headless + remote control
tark --headless --remote discord
```

5) Use slash commands in Discord

Examples:
- `/tark status`
- `/tark usage`
- `/tark mode ask`
- `/tark trust manual`
- `/tark stop`
- `/tark resume`
- `/tark interrupt`

## Allowlists and Control Policy

Remote requests are gated by allowlists in `.tark/config.toml`.

```toml
[remote]
http_enabled = false
allowed_plugins = ["discord"]
allowed_users = ["1234567890"]
allowed_guilds = ["0987654321"]
allowed_channels = ["55555555"]
allowed_roles = ["role-id"]
allow_model_change = true
allow_mode_change = true
allow_trust_change = false
require_allowlist = true
```

If `require_allowlist` is true, at least one allowlist must be populated or all remote messages are rejected.

## Observability

- Live events are shown in the remote TUI (left: sessions, right: events)
- Headless mode prints events to stdout
- Rolling logs are written to `.tark/logs/remote` (error-only by default)
- Use `--remote-debug` to log full events

## Queues and Interrupts

- When the agent is busy, incoming remote messages are queued (you'll get a queue position).
- Use `/tark interrupt` to stop the current task at the next safe checkpoint.

## Plugin Widgets

Channel plugins can expose a **widget JSON** that Tark renders in the sidebar (normal TUI) and in the remote TUI.
The refresh interval is controlled by:

```toml
[tui]
plugin_widget_poll_ms = 2000
```

## Session Management

```bash
tark show all
tark show <session-id>
tark stop <session-id>
tark resume <session-id>
```

## OAuth (Optional)

The Discord plugin includes an OAuth manifest for installing the app via OAuth2 (PKCE). If you use it:

```bash
export DISCORD_CLIENT_ID="<client-id>"
export DISCORD_CLIENT_SECRET="<client-secret>"
export DISCORD_REDIRECT_URI="http://localhost:8888/callback"

# Run OAuth flow
tark plugin auth discord
```

If you only need Gateway + bot token, OAuth is not required.

When you run `tark plugin auth discord`, the CLI will prompt for:
- Discord Application ID
- Discord Public Key
- (Optional) Bot Token
- Storage scope (global or project-local `.tark`)

Credentials are encrypted at rest. Set `TARK_PLUGIN_PASSPHRASE` to avoid prompts in headless environments.

## Private Mode

The Discord plugin runs in **DM-only** mode by default. Guild requests are rejected and users are asked to DM the bot instead.
