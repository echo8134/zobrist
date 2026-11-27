import { execFileSync } from "node:child_process";
import { mkdtempSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";
import { answerNextPicker } from "./file-picker";

const root = fileURLToPath(new URL("../../../../", import.meta.url));
const main = () => browser.$("main");
const entry = () => main().$("button=Masters.zobrist");
const count = () => main().$("span=2 games");

function databaseCommand(...args: string[]) {
  return execFileSync(
    "cargo",
    [
      "run",
      "--quiet",
      "--locked",
      "-p",
      "zobrist-cli",
      "--",
      "database",
      ...args,
    ],
    { cwd: root, encoding: "utf8" },
  );
}

async function choose(path: string | null) {
  await answerNextPicker("PickDatabasePath", path);
  await main().$("aria/Add collection…").click();
  await browser.waitUntil(async () =>
    Boolean(await browser.execute("return window.pickerRequest")),
  );
}

async function showDatabases() {
  await browser.$("nav").$("aria/Databases").click();
  await expect(main().$("h1")).toHaveText("Databases");
}

describe("local databases", () => {
  it("remembers verified collections and removes only the reference", async () => {
    const directory = mkdtempSync(join(tmpdir(), "zobrist-databases-"));
    const database = join(directory, "Masters.zobrist");
    const invalid = join(directory, "Invalid.zobrist");
    try {
      databaseCommand(
        "create",
        "crates/cli/tests/fixtures/clean.pgn",
        database,
      );
      writeFileSync(invalid, "invalid");
      await showDatabases();
      await choose(database);
      await expect(entry()).toBeDisplayed();
      await expect(main().$("h2=This device")).toBeDisplayed();
      await expect(main().$(`p=${database}`)).toBeDisplayed();
      await expect(count()).toBeDisplayed();
      const picture = await browser.saveScreenshot(
        join(root, "target/e2e/databases-local.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);

      await choose(database);
      await choose(null);
      await browser.refresh();
      await showDatabases();
      await expect(main().$$("button=Masters.zobrist")).toBeElementsArrayOfSize(
        1,
      );
      await entry().click();
      await expect(count()).toBeDisplayed();

      await choose(invalid);
      await expect(main().$('[role="status"]')).toHaveText(
        "Not a zobrist database",
      );
      await expect(main().$("button=Invalid.zobrist")).not.toExist();
      const moved = join(directory, "Moved.zobrist");
      renameSync(database, moved);
      await entry().click();
      await expect(main().$('[role="status"]')).toHaveText("File unavailable");
      await expect(count()).not.toExist();
      renameSync(moved, database);
      await entry().click();
      await expect(count()).toBeDisplayed();

      await main().$("aria/Remove from list").click();
      await expect(entry()).not.toExist();
      await browser.refresh();
      await showDatabases();
      await expect(main().$("p=No collections yet.")).toBeDisplayed();
      await expect(entry()).not.toExist();
      expect(databaseCommand("info", database)).toBe("games: 2\n");
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
