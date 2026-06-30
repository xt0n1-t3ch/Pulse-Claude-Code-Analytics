# Discord Rich Presence assets

Discord **only reliably renders Rich Presence images that live in the app's
Developer Portal**. Plain `https://...` URLs get silently dropped by most
Discord client versions. If your Pulse install shows `Claude Code` in your
Discord status but no logo, this is the fix.

## Background

Before schema v3, the default config pointed `large_image_key` at a raw
GitHub URL that was never actually committed to `origin/main`, so Discord
resolved it to a 404 and showed nothing. Schema v3 changed the default to the
asset key `"claude-code"` — which works _if_ an asset by that name is uploaded
to the client-id's portal.

## Upload checklist

1. Open the app in the Developer Portal:
   <https://discord.com/developers/applications/1466664856261230716/rich-presence/assets>
   (replace the id if you've configured a custom `CC_DISCORD_CLIENT_ID`).
2. Click **Add Image(s)** and upload the files listed below, using the file
   **stem** as the key (Discord lowercases it automatically).

| Asset key       | Source file                                               | Used for                      |
| --------------- | --------------------------------------------------------- | ----------------------------- |
| `claude-code`   | `assets/branding/claude-mascot.jpg` (or any 1024² logo)    | `large_image` (main artwork)  |
| `thinking`      | 512² icon — Claude thinking                               | Small badge when reasoning    |
| `reading`       | 512² icon — file/eye                                      | Small badge when reading      |
| `editing`       | 512² icon — pencil                                        | Small badge when editing      |
| `running`       | 512² icon — terminal                                      | Small badge when running cmd  |
| `waiting`       | 512² icon — hourglass                                     | Small badge when idle-waiting |
| `idle`          | 512² icon — muted                                         | Small badge when idle         |

3. Click **Save Changes**.

### Codex apps

Codex broadcasts from two separate Discord applications by surface, each of which
needs its own upload. As of this release neither has assets uploaded yet, so the
live Codex presence shows no large image until you add them (the in-app preview
is unaffected — see below). Tracked in issue #36.

| Application      | Client ID            | Portal                                                                                          | Asset key     | Source file                                            |
| ---------------- | -------------------- | ----------------------------------------------------------------------------------------------- | ------------- | ------------------------------------------------------ |
| Codex CLI / VS Code | `1470480085453770854` | <https://discord.com/developers/applications/1470480085453770854/rich-presence/assets> | `codex-logo`  | the Codex mark (e.g. `assets/branding/codex-app.png`)  |
| Codex App / OpenCode | `1478395304624652345` | <https://discord.com/developers/applications/1478395304624652345/rich-presence/assets> | `codex-app`   | `assets/branding/codex-app.png`                        |

The Codex service tier (Fast mode) and surface (App vs CLI) are detected from the
local Codex state — see [plan-detection.md](plan-detection.md).

## In-app Live Preview art

The Pulse **Discord** view bundles the real Rich Presence artwork locally
(`frontend/src/assets/rp/`, mapped by provider/surface in
`frontend/src/lib/rpArt.ts`) so the Live Preview is faithful **regardless of the
Developer Portal**. This is preview-only; it does not change what Discord
broadcasts, which still depends on the uploads above.

## Fallback tiers (no manual upload required)

If the asset key isn't in the portal, Pulse falls back through three tiers
automatically (`src/discord.rs::resolve_image_ref`):

1. **Key in portal** — the reliable path. Use asset keys uploaded above.
2. **`mp:` prefix** — pass through any pre-computed Discord media reference.
3. **`https://` URL** — wrapped as `mp:external/https/<host>/<path>`, relying
   on Discord's Media Proxy. Works sometimes, not guaranteed.
4. **Raw key** — passed straight to Discord; if the key doesn't exist, Discord
   silently drops the image.

## Diagnosing a missing logo

From the Pulse GUI, open **Settings** → the Discord diagnostic panel shows:
- Which resolution tier is currently active
- Known asset keys fetched from the portal (refreshes every 5 min)
- Suggested fix if the active key isn't in the portal

From the CLI:
```bash
cc-discord-presence doctor
```
Look at `discord_client_id` and the active session's resolved `large_image`.

## Customising assets per user

Users can override any activity key via the config file
(`~/.claude/discord-presence-config.json`):

```jsonc
{
  "display": {
    "large_image_key": "my-custom-logo",
    "activity_small_image_keys": {
      "thinking": "my-thinking-icon",
      "editing": "my-editing-icon"
    }
  }
}
```

Any key you reference must exist in the Developer Portal for the
configured `discord_client_id`.
