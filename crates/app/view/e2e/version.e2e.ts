import { mkdirSync, readFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";

const screenshot = fileURLToPath(
  new URL("../../../../target/e2e/version.png", import.meta.url),
);

// A release changes only the version line of the root manifest, so the expected value comes
// from there and not from a literal.
const manifest = readFileSync(
  new URL("../../../../Cargo.toml", import.meta.url),
  "utf8",
);
const version = /\[workspace\.package\][^[]*?^version = "([^"]+)"$/m.exec(
  manifest,
)?.[1];
if (version === undefined) {
  throw new Error("[workspace.package] of the root manifest sets no version");
}

describe("the window", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
  });

  it("shows the version that Rust answered", async () => {
    await expect(browser.$("body")).toHaveText(`zobrist ${version}`);

    mkdirSync(dirname(screenshot), { recursive: true });
    await browser.saveScreenshot(screenshot);
  });
});
