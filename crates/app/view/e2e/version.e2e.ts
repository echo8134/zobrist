import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";

const screenshot = fileURLToPath(
  new URL("../../../../target/e2e/version.png", import.meta.url),
);

describe("the window", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
  });

  it("shows the version that Rust answered", async () => {
    await expect(browser.$("body")).toHaveText("zobrist 0.0.0");

    mkdirSync(dirname(screenshot), { recursive: true });
    await browser.saveScreenshot(screenshot);
  });
});
