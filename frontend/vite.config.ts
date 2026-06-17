import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

const frontendRoot = __dirname;

export default defineConfig({
  root: frontendRoot,
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(frontendRoot, "./src"),
    },
  },
  build: {
    outDir: path.resolve(frontendRoot, "../dist"),
    emptyOutDir: true,
    chunkSizeWarningLimit: 700,
  },
  server: {
    strictPort: true,
    port: 1420,
    host: "127.0.0.1",
    watch: {
      ignored: ["**/src-tauri/**", "**/crates/**", "**/cli/**"],
    },
  },
});
