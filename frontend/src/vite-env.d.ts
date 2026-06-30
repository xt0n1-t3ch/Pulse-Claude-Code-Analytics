/// <reference types="svelte" />
/// <reference types="vite/client" />

declare global {
  interface Window {
    __TAURI__?: {
      window: typeof import("@tauri-apps/api/window");
    };
  }
}

export {};
