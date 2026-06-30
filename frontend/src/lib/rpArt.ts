// Real Rich Presence art, bundled and mapped by provider/surface so the Live
// Preview renders the same large image Discord shows. Claude Code uses the
// official mascot; Codex uses the official Codex mark (the blue blossom with the
// terminal prompt). Small badge is null until a distinct small asset exists, so
// the card matches Discord's large-image-only rendering rather than duplicating
// the large art.
import claudeCode from "../assets/rp/claude-code.png";
import codexApp from "../assets/rp/codex-app.png";

export interface RpArt {
  /** Large image URL (bundled). */
  large: string;
  /** Small overlay badge URL, or null when there is no distinct small asset. */
  small: string | null;
  /** Hover text for the large image (Discord `large_text`). */
  largeText: string;
  /** Asset key Discord would request for the large image. */
  assetKey: string;
}

export function rpArtFor(provider: string, appName?: string | null): RpArt {
  if (provider === "codex") {
    const isApp = appName === "Codex App";
    return {
      large: codexApp,
      small: null,
      largeText: isApp ? "Codex App" : "Codex CLI / VS Code",
      assetKey: isApp ? "codex-app" : "codex-logo",
    };
  }
  return {
    large: claudeCode,
    small: null,
    largeText: "Claude Code",
    assetKey: "claude-code",
  };
}
