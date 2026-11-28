import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const { ZOBRIST_TEST_DATA: data } = process.env;
if (!data) throw new Error("The window test needs its own data directory.");
const directory = join(data, "io.github.echo8134.zobrist");
const engines = join(directory, "engines.json");
const accounts = join(directory, "accounts.json");
const broken = Buffer.from("{broken");
const accountMessage = `Could not read the account list at ${accounts}. Your PGN files and online accounts are untouched. Restore the list from a backup or move it aside, then reopen User.`;
const alert = () => browser.$('main [role="alert"]');

async function screenshot(name: string) {
  const picture = await browser.saveScreenshot(join(screenshots, name));
  expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
    1280, 800,
  ]);
}

describe("damaged local lists", () => {
  beforeEach(async () => {
    await browser.$("nav").$("aria/Home").click();
    mkdirSync(directory, { recursive: true });
  });

  after(() => {
    rmSync(engines, { force: true });
    rmSync(accounts, { force: true });
  });

  it("shows how to recover the engine list and preserves it when adding an engine", async () => {
    writeFileSync(engines, broken);
    const message = `Could not read the engine list at ${engines}. Your engine programs are untouched. Restore the list from a backup or move it aside, then reopen Engines.`;
    await browser.$("nav").$("aria/Engines").click();
    await expect(alert()).toHaveText(message);
    await pick("Engine", process.execPath);
    await expect(browser.$("aria/Add engine…")).toBeEnabled();
    await expect(alert()).toHaveText(message);
    expect(readFileSync(engines)).toEqual(broken);
    await screenshot("damaged-engines.png");
  });

  it("shows how to recover the account list on opening User and keeps Save disabled", async () => {
    writeFileSync(accounts, broken);
    await browser.$("nav").$("aria/User").click();
    await expect(alert()).toBeDisplayed();
    await expect(alert()).toHaveText(accountMessage);
    await screenshot("damaged-accounts.png");
    await browser.$("main summary").click();
    await expect(browser.$("button=Save")).toBeDisabled();
    expect(readFileSync(accounts)).toEqual(broken);
  });

  it("preserves an account list damaged while User is open", async () => {
    writeFileSync(accounts, '{"lichessUsername":"OldName"}');
    await browser.$("nav").$("aria/User").click();
    await browser.$("main summary").click();
    const field = browser.$("aria/Lichess username");
    await expect(field).toHaveValue("OldName");
    await field.setValue("NewName");
    writeFileSync(accounts, broken);
    await browser.$("button=Save").click();
    await expect(field).toBeEnabled();
    expect(readFileSync(accounts)).toEqual(broken);
    await expect(alert()).toHaveText(accountMessage);
  });
});
