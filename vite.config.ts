import { defineConfig } from "vite";

// Tauri espera el servidor de desarrollo en un puerto fijo.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
