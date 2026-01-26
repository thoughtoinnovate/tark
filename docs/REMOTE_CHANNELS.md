# Remote Channels

Tark can run channel plugins (Discord/Slack/etc.) in remote mode so you can drive sessions from chat while keeping a local TUI or headless watcher.

## Quick Start (Discord)

1) Create a Discord application (Developer Portal)
- Add a Bot user
- Copy the Bot token
- Copy the Application ID and Public Key

2) Expose the webhook endpoint
- Tark listens on: `http://<host>:<port>/channels/discord/webhook`
- For local dev, tunnel it (ngrok/Cloudflare Tunnel) and set the public URL in Discord's Interactions endpoint

3) Set env vars

```bash
export DISCORD_PUBLIC_KEY="<public-key>"
export DISCORD_APPLICATION_ID="<app-id>"
export DISCORD_BOT_TOKEN="<bot-token>"
export DISCORD_PRIVATE_MODE="dm" # dm | ephemeral | off
```

4) Start remote mode

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

## Allowlists and Control Policy

Remote requests are gated by allowlists in `.tark/config.toml`.

```toml
[remote]
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

If you only need interactions + bot token, OAuth is not required.

## Private Mode

`DISCORD_PRIVATE_MODE` controls privacy behavior for guild channels:
- `dm` (default): Rejects guild requests and asks users to DM the bot.
- `ephemeral`: Allows guild requests but responses are ephemeral (visible only to the user).
- `off`: Allows normal guild responses (public).
