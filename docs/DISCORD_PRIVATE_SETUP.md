# Discord Private Setup (DM‑Only)

This guide configures a **private Discord bot** that only you can use, with Tark running on your machine. Messages are **DM‑only**, and credentials are **encrypted at rest**. The plugin uses the **Discord Gateway (WebSocket)**, so **no public webhook / ngrok** is required.

## 1) Create a Discord app
1. Go to https://discord.com/developers/applications  
2. **New Application** → name it (e.g. `tark`)  
3. **Bot** tab → **Add Bot**

### Bot settings (privacy)
- **Public Bot**: **OFF**  
- **Requires OAuth2 Code Grant**: **OFF**  
- **Privileged Gateway Intents**:  
  - **Message Content Intent**: **ON** (required for plain text DMs)  
  - **Presence Intent**: **OFF**  
  - **Server Members Intent**: **OFF**

These keep the bot private and prevent unnecessary access.

## 2) (Optional) Redirect URL
Only needed if you plan to use OAuth later.
In **OAuth2 → General**, add:
```
http://localhost:8888/callback
```
Save changes.

## 3) Generate install URL
In **OAuth2 → URL Generator**:
- **Scopes**:  
  - `applications.commands`  
  - `bot`
- **Bot Permissions**: **none** (DM‑only)

Open the generated URL once to authorize/install.

## 4) Install the plugin
```
tark plugin add git@github.com:thoughtoinnovate/plugins.git --path tark/discord
```

## 5) Authenticate + store secrets (encrypted)
```
tark plugin auth discord
```
You will be prompted for:
- Discord **Application ID**
- Discord **Public Key**
- (Optional) **Bot Token**
- Storage scope:  
  - **Global** (shared across projects)  
  - **Project** (stored under `.tark/`)
- Encryption passphrase (used to encrypt credentials)

### Headless mode
Set this to skip prompts:
```
export TARK_PLUGIN_PASSPHRASE="your-passphrase"
```

## 6) Allowlist only your Discord user
In `.tark/config.toml`:
```toml
[remote]
http_enabled = false
allowed_plugins = ["discord"]
allowed_users = ["<your-discord-user-id>"]
require_allowlist = true
```

## 7) Run Tark (DM‑only, Gateway mode)
```
tark --remote discord
```

The Discord plugin is **forced DM‑only**. Any server message is rejected with an ephemeral “please DM” response.

## 8) Use it in Discord
Open a DM with the bot and run:
```
/tark status
/tark usage
/tark interrupt
```

---

## Notes
- **App ID** + **Public Key** are not secret.  
- **Bot Token** and **Client Secret** are secret.  
- Credentials are encrypted at rest and only decrypted with your passphrase.  
