import { fileURLToPath } from "node:url";
import type { TauriCapabilities } from "@wdio/tauri-service";

const target = fileURLToPath(new URL("../../../target", import.meta.url));

const app: TauriCapabilities = {
  browserName: "tauri",
  "tauri:options": { application: `${target}/debug/zobrist-app` },
};

export const config: WebdriverIO.Config = {
  runner: "local",
  // The caller provides the display. Without this line the runner would start one of its own.
  autoXvfb: false,
  specs: ["./e2e/*.e2e.ts"],
  framework: "mocha",
  reporters: ["spec"],
  logLevel: "warn",
  capabilities: [app],
  services: [
    [
      "@wdio/tauri-service",
      {
        // The app under test keeps its data out of the home directory.
        env: {
          XDG_DATA_HOME: `${target}/e2e/data`,
          XDG_CACHE_HOME: `${target}/e2e/cache`,
        },
      },
    ],
  ],
};
