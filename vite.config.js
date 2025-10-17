import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from "fs";

const host = process.env.TAURI_DEV_HOST;

// 从 package.json 读取版本号
const packageJson = JSON.parse(readFileSync("./package.json", "utf-8"));
const version = packageJson.version;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(version),
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
