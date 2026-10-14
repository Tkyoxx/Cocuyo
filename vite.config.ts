import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "esnext",
    modulePreload: false,
    rollupOptions: {
      input: { index: "index.html", fondo: "fondo.html" },
    },
  },
});
