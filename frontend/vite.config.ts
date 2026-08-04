import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig(() => {
  const bridgeToken = process.env.PULSE_DEV_BRIDGE_TOKEN;

  return {
    plugins: [svelte()],
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      proxy: bridgeToken
        ? {
            "/__pulse_api": {
              target: "http://127.0.0.1:1421",
              changeOrigin: false,
              rewrite: () => "/invoke",
              configure(proxy) {
                proxy.on("proxyReq", (request) => {
                  request.setHeader("Authorization", `Bearer ${bridgeToken}`);
                });
              },
            },
          }
        : undefined,
    },
    build: {
      target: "esnext",
      outDir: "dist",
    },
  };
});
