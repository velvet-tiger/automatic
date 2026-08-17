/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1421,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1422,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri` and the per-agent sync
      //    output directories that Automatic writes to when syncing projects.
      //    Without this, editing a rule (or any library asset) while running
      //    `make dev` against the automatic-app project would trigger Vite
      //    HMR and reload the frontend, losing all in-memory state.
      ignored: [
        "**/src-tauri/**",
        "**/.automatic/**",
        "**/.agents/**",
        "**/.claude/**",
        "**/.cline/**",
        "**/.clinerules/**",
        "**/.codex/**",
        "**/.cursor/**",
        "**/.cursorrules/**",
        "**/.factory/**",
        "**/.gemini/**",
        "**/.goosehints/**",
        "**/.junie/**",
        "**/.kilocode/**",
        "**/.kiro/**",
        "**/.antigravity/**",
        "**/.opencode/**",
        "**/.warp/**",
        "**/.zcode/**",
        "**/.zed/**",
        "**/AGENTS.md",
        "**/CLAUDE.md",
        "**/CODEX.md",
      ],
    },
  },
  test: {
    environment: "happy-dom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    css: false,
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
  },
}));
