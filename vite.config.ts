import { defineConfig } from "vite"

export default defineConfig({
  // The window ships inside WKWebView on macOS 14+ and WebView2 (Chromium) on
  // Windows 10/11, so there is no older engine to down-level for.
  build: { target: ["safari17", "chrome120"], cssMinify: "lightningcss" },
})
