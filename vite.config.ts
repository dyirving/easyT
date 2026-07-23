import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

// Tauri 期望前端在固定端口上提供 dev server
const HOST = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],
  // Tauri 使用固定端口，避免每次构建端口变动导致连接失败
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: HOST || false,
    hmr: HOST
      ? {
          protocol: "ws",
          host: HOST,
          port: 1421,
        }
      : undefined,
    watch: {
      // 忽略 Rust 目录，避免触发不必要的前端热更新
      ignored: ["**/src-tauri/**"],
    },
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
}));
