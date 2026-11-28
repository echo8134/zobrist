import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { expect } from "@wdio/globals";
import { visitPage } from "./desktop-session";
import { importPgn } from "./file-picker";

const root = fileURLToPath(new URL("../../../../", import.meta.url));
const fixture = join(root, "crates/cli/tests/fixtures/browse.pgn");
const table = 'table[aria-label="Collection games"]';

function field(app: WebdriverIO.Browser, label: string) {
  return app.$(
    `//main//label[normalize-space(text())="${label}"]/*[self::input or self::select]`,
  );
}

async function chooseResult(app: WebdriverIO.Browser, result: string) {
  // The native select menu is outside the web view.
  await app.execute(
    `const select = arguments[0];
     select.value = arguments[1];
     select.dispatchEvent(new Event("change", { bubbles: true }));`,
    await field(app, "Result").getElement(),
    result,
  );
}

async function expectNumbers(app: WebdriverIO.Browser, numbers: number[]) {
  await app.waitUntil(
    async () => {
      const actual = await app.execute<number[], [string]>(
        `return Array.from(document.querySelectorAll(arguments[0]), cell => Number(cell.textContent))`,
        `${table} tbody tr td:first-child`,
      );
      return isDeepStrictEqual(actual, numbers);
    },
    { timeoutMsg: `Expected game numbers ${numbers.join(", ")}` },
  );
}

describe("games in a collection", () => {
  let directory: string;
  let browse: string;

  beforeEach(() => {
    directory = mkdtempSync(join(tmpdir(), "zobrist-games-"));
    browse = join(directory, "Browse.zobrist");
  });
  afterEach(() => rmSync(directory, { recursive: true, force: true }));

  function onCollection(check: (app: WebdriverIO.Browser) => Promise<void>) {
    return visitPage(directory, "Databases", async (app) => {
      await expect(app.$("button=Import PGN…")).toBeEnabled();
      await importPgn(app, fixture, browse);
      await app.$("button=Browse.zobrist").click();
      await expectNumbers(app, [1, 2, 3, 4]);
      await check(app);
    });
  }

  it("sorts by White in both directions, keeping equal names in game order", async () => {
    await onCollection(async (app) => {
      const white = app.$(table).$("button=White");
      await white.click();
      await expectNumbers(app, [1, 4, 3, 2]);
      await white.click();
      await expectNumbers(app, [2, 3, 1, 4]);
    });
  });

  it("combines filters, clears an empty result and finds an unfinished game", async () => {
    await onCollection(async (app) => {
      await field(app, "Player").setValue("ADa");
      await expectNumbers(app, [1, 2, 4]);
      await field(app, "Event").setValue("arbor");
      await expectNumbers(app, [1, 2]);
      await field(app, "Year").setValue("2025");
      await chooseResult(app, "0-1");
      await expectNumbers(app, [2]);
      const cells = app.$$(`${table} tbody td`);
      expect(await cells.map((cell) => cell.getText())).toEqual([
        "2",
        "Cora",
        "Ada",
        "0-1",
        "2025.03.02",
        "Harbor Open",
      ]);
      const picture = await app.saveScreenshot(
        join(root, "target/e2e/database-games.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);

      await chooseResult(app, "1-0");
      await expect(
        app.$('//main//*[text()="No matching games"]'),
      ).toBeDisplayed();
      await app.$("button=Clear filters").click();
      await expectNumbers(app, [1, 2, 3, 4]);
      await chooseResult(app, "*");
      await expectNumbers(app, [4]);
      await expect(app.$(`${table} tbody td:nth-child(5)`)).toHaveText("?");
    });
  });

  it("opens the filtered game's main line, comment and variation and preserves its collection", async () => {
    await onCollection(async (app) => {
      const original = readFileSync(browse);
      await chooseResult(app, "0-1");
      await expectNumbers(app, [2]);
      await app.$(`${table} tbody tr`).click();
      await expect(
        app.$('header [role="tab"][aria-selected="true"]'),
      ).toHaveText("Cora - Ada");
      await expect(app.$("main cg-board")).toBeDisplayed();
      const line = app.$$("main aside ol:not(ol ol) > li > span > button");
      expect(await line.map((move) => move.getText())).toEqual([
        "d4",
        "d5",
        "c4",
      ]);
      await expect(app.$("main aside").$("span=Main reply.")).toBeDisplayed();
      const variation = app.$("main aside ol ol");
      await expect(variation.$("li > span > span")).toHaveText("1...");
      await expect(variation.$("button")).toHaveText("Nf6");
      expect(readFileSync(browse)).toEqual(original);
    });
  });

  it("pages through 101 games, finds game 101 from the first page and switches collections", async () => {
    await onCollection(async (app) => {
      const source = join(directory, "League.pgn");
      const game = (white: string) =>
        `[White "${white}"]\n[Result "*"]\n\n1. e4 *\n\n`;
      writeFileSync(source, game("Ada").repeat(100) + game("Vega"));
      await importPgn(app, source, join(directory, "League.zobrist"));
      const firstPage = Array.from({ length: 100 }, (_, index) => index + 1);
      await expectNumbers(app, firstPage);
      await app.$("button=Next").click();
      await expectNumbers(app, [101]);
      await app.$("button=Previous").click();
      await expectNumbers(app, firstPage);
      await field(app, "Player").setValue("Vega");
      await expectNumbers(app, [101]);
      await app.$("button=Browse.zobrist").click();
      await expectNumbers(app, [1, 2, 3, 4]);
    });
  });
});
