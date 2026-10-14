import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// A relative base lets the built site be served from any path, including a
// project page such as user.github.io/repo/.
export default defineConfig({
  base: "./",
  plugins: [react()],
  build: {
    target: "es2022",
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        manualChunks: {
          three: ["three"],
          katex: ["katex"],
        },
      },
    },
  },
  server: { port: 5173, strictPort: true },
});
