import { rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { TauriCapabilities } from "@wdio/tauri-service";

const target = fileURLToPath(new URL("../../../target", import.meta.url));

const data = `${target}/e2e/data/${process.pid}`;

const app: TauriCapabilities = {
  browserName: "tauri",
  "tauri:options": { application: `${target}/debug/zobrist-app` },
};

export const config: WebdriverIO.Config = {
  runner: "local",
  runnerEnv: { ZOBRIST_TEST_DATA: data },
  // The caller provides the display. Without this line the runner would start one of its own.
  autoXvfb: false,
  specs: ["./e2e/*.e2e.ts"],
  // The service starts one app for the whole run, so the spec files use it one after the other.
  maxInstances: 1,
  // A refresh restores open boards, so close the preceding spec’s tabs first.
  before: async (_capabilities, _specs, browser) => {
    await browser.switchToWindow("main");
    const tabs = () => browser.$$('header [role="tab"]');
    for (let count = await tabs().length; count > 0; count--) {
      const first = browser.$('header [role="tablist"] > :first-child');
      const discard = first.$("button=Close");
      if (!(await discard.isExisting())) {
        await first.$('[aria-label^="Close "]').click();
        await browser.waitUntil(
          async () =>
            (await tabs().length) < count || (await discard.isExisting()),
        );
      }
      if ((await tabs().length) === count) await discard.click();
      await browser.waitUntil(async () => (await tabs().length) === count - 1);
    }
    await browser.refresh();
  },
  onComplete: () => {
    rmSync(data, { recursive: true, force: true });
  },
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
          XDG_DATA_HOME: data,
          XDG_CACHE_HOME: `${target}/e2e/cache`,
        },
      },
    ],
  ],
};
