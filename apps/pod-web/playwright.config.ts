import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  testMatch: /.*\.e2e\.ts/,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: "line",
  use: {
    baseURL: "http://127.0.0.1:4178",
    headless: true,
    launchOptions: {
      args: [
        "--use-angle=swiftshader",
        "--use-gl=angle",
        "--enable-unsafe-webgpu"
      ]
    },
    viewport: {
      width: 1440,
      height: 900
    }
  },
  webServer: {
    command: "bun run dev --host 127.0.0.1 --port 4178",
    url: "http://127.0.0.1:4178",
    reuseExistingServer: true,
    timeout: 30_000
  }
});
