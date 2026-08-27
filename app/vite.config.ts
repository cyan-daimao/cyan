import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],
  // Tauri 开发约定：固定端口与严格模式
  clearScreen: false,
  build: {
    // 桌面应用本地加载，antd vendor 包 ~1MB 属正常
    chunkSizeWarningLimit: 1100,
    rollupOptions: {
      output: {
        // antd 体积较大，拆为独立 vendor chunk
        manualChunks: {
          react: ["react", "react-dom", "react-router-dom"],
          antd: ["antd", "@ant-design/icons"],
        },
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
