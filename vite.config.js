import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  // Cargo writes and locks files under src-tauri/target; watching them crashes Vite with EBUSY.
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: { target: "es2021", outDir: "dist" },
});
