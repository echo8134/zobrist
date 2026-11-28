import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  utimesSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { visitPage } from "./desktop-session";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const ada = '[White "Ada"]\r\n\r\n1. e4 {keep  this} *\r\n';
const bea = '[White "Bea"]\n[Black "Bo"]\n\n1. d4 *\n';
const cy = '[White "Cy"]\n\n1. c4 *';
const original = `${ada}\r\n${bea}\n${cy}`;
const withoutBea = `${ada}\r\n${cy}`;
const files = (session: WebdriverIO.Browser) =>
  session.$("nav").$("aria/Files").click();
const dialog = (session: WebdriverIO.Browser) => session.$("dialog[open]");
const history = (session: WebdriverIO.Browser) =>
  session.$('section[aria-label="Earlier states"]');
const earlier = (session: WebdriverIO.Browser) =>
  session.$("main").$("button=Earlier states");
const remove = (session: WebdriverIO.Browser, number: number) =>
  session.$(`main table button[aria-label="Delete game ${number}"]`).click();
const restore = async (session: WebdriverIO.Browser, number: number) => {
  await history(session).$(`aria/State ${number}`).click();
  await dialog(session).$("button=Restore").click();
};
const confirm = (session: WebdriverIO.Browser) =>
  dialog(session).$("button=Delete").click();

async function checkRows(
  session: WebdriverIO.Browser,
  white: readonly string[],
) {
  const rows = session.$$("main table tbody tr");
  await expect(rows).toBeElementsArrayOfSize(white.length);
  const cells: string[][] = [];
  for (const row of await rows.getElements()) {
    cells.push(
      await row.$$("td:nth-child(-n+2)").map((cell) => cell.getText()),
    );
  }
  expect(cells).toEqual(white.map((name, index) => [String(index + 1), name]));
}

async function checkHistory(session: WebdriverIO.Browser, numbers: number[]) {
  if ((await earlier(session).getAttribute("aria-expanded")) !== "true")
    await earlier(session).click();
  const panel = history(session);
  if (numbers.length === 0) {
    await expect(panel.$("p=No earlier states.")).toBeDisplayed();
  } else {
    await expect(panel.$("li:first-child button")).toHaveAttribute(
      "aria-label",
      `State ${numbers[0]}`,
    );
  }
  const buttons = panel.$$("li button");
  await expect(buttons).toBeElementsArrayOfSize(numbers.length);
  expect(
    await buttons.map((button) => button.getAttribute("aria-label")),
  ).toEqual(numbers.map((number) => `State ${number}`));
}

async function expectRefusal(session: WebdriverIO.Browser, text: string) {
  const message = session
    .$('main section[aria-label="Selection"]')
    .$(`p=${text}`);
  await session.waitUntil(
    async () =>
      (await dialog(session).isDisplayed()) || (await message.isDisplayed()),
  );
  if (await dialog(session).isDisplayed()) await confirm(session);
  await expect(message).toBeDisplayed();
}

describe("deleting one PGN game on Files", function () {
  this.timeout(60_000);
  let work: string;
  let path: string;
  const bytes = () => readFileSync(path, "utf8");
  beforeEach(() => {
    work = mkdtempSync(join(tmpdir(), "zobrist-delete-"));
    path = join(work, "Club.pgn");
  });
  afterEach(() => rmSync(work, { recursive: true, force: true }));

  async function openClub(
    pgn: string,
    run: (session: WebdriverIO.Browser) => Promise<void>,
  ) {
    writeFileSync(path, pgn);
    // Whole seconds can be restored exactly when the stale-file checks edit a byte.
    utimesSync(path, 1_700_000_000, 1_700_000_000);
    await visitPage(work, "Files", async (session) => {
      await pick("File", path, session);
      await expect(session.$("main").$("h2=Club.pgn")).toBeDisplayed();
      await run(session);
    });
  }

  it("confirms the named game, restores every byte and requires a recoverable copy before deleting", async () => {
    await openClub(original, async (session) => {
      await checkRows(session, ["Ada", "Bea", "Cy"]);
      await remove(session, 2);
      await expect(dialog(session).$("h2")).toHaveText(
        `Delete game 2 (Bea - Bo) from ${path}?`,
      );
      expect(
        await dialog(session)
          .$$("button")
          .map((button) => button.getText()),
      ).toEqual(["Cancel", "Delete"]);
      expect(bytes()).toBe(original);
      mkdirSync(screenshots, { recursive: true });
      const picture = await session.saveScreenshot(
        join(screenshots, "files-delete-game.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
      await dialog(session).$("button=Cancel").click();
      await expect(dialog(session)).not.toExist();
      expect(bytes()).toBe(original);
      await checkRows(session, ["Ada", "Bea", "Cy"]);
      await checkHistory(session, []);

      await remove(session, 2);
      await confirm(session);
      await expect(session.$("main").$("span=2 games")).toBeDisplayed();
      await checkRows(session, ["Ada", "Cy"]);
      expect(bytes()).toBe(withoutBea);
      await checkHistory(session, [1]);
      await restore(session, 1);
      await checkRows(session, ["Ada", "Bea", "Cy"]);
      expect(bytes()).toBe(original);
      await checkHistory(session, [2, 1]);

      const root = join(
        work,
        "data",
        "io.github.echo8134.zobrist",
        "pgn-history",
      );
      const [folder] = readdirSync(root, { withFileTypes: true }).filter(
        (entry) => entry.isDirectory(),
      );
      if (!folder) throw Error("Expected a history for Club.pgn");
      const historyPath = join(root, folder.name);
      const first = join(historyPath, "1.state");
      const second = join(historyPath, "2.state");
      const kept = [readFileSync(first), readFileSync(second)];
      chmodSync(historyPath, 0o555);
      try {
        await remove(session, 2);
        await expectRefusal(
          session,
          `Could not delete game 2. The current bytes could not be kept in ${historyPath}. The history folder is read-only. Allow writing to this folder and try again.`,
        );
        expect(bytes()).toBe(original);
        await checkRows(session, ["Ada", "Bea", "Cy"]);
        await checkHistory(session, [2, 1]);
        expect([readFileSync(first), readFileSync(second)]).toEqual(kept);
      } finally {
        chmodSync(historyPath, 0o755);
      }
      await remove(session, 2);
      await confirm(session);
      await checkRows(session, ["Ada", "Cy"]);
      expect(bytes()).toBe(withoutBea);
      await checkHistory(session, [3, 2, 1]);
      await restore(session, 3);
      await checkRows(session, ["Ada", "Bea", "Cy"]);
      expect(bytes()).toBe(original);
      await restore(session, 2);
      await checkRows(session, ["Ada", "Cy"]);
      expect(bytes()).toBe(withoutBea);
    });
  });

  for (const [place, pgn, number, remaining, white] of [
    ["first", original, 1, `${bea}\n${cy}`, ["Bea", "Cy"]],
    ["last", original, 3, `${ada}\r\n${bea}`, ["Ada", "Bea"]],
    ["only", bea, 1, "", []],
  ] as const) {
    it(`deletes the ${place} game and preserves the surrounding PGN bytes`, async () => {
      await openClub(pgn, async (session) => {
        await remove(session, number);
        await confirm(session);
        await checkRows(session, white);
        expect(bytes()).toBe(remaining);
        if (place === "only") {
          await expect(
            session.$('section[aria-label="Collection"]').$("button=Club.pgn"),
          ).toBeDisplayed();
          await expect(session.$("main").$("span=0 games")).toBeDisplayed();
          await checkHistory(session, [1]);
          await restore(session, 1);
          await checkRows(session, ["Bea"]);
          expect(bytes()).toBe(bea);
        }
      });
    });
  }

  for (const change of ["before", "after"] as const) {
    it(`refuses an edit with the same size and timestamp ${change} the confirmation opens`, async () => {
      await openClub(original, async (session) => {
        await checkRows(session, ["Ada", "Bea", "Cy"]);
        if (change === "after") {
          await remove(session, 2);
          await expect(dialog(session)).toBeDisplayed();
        }
        const before = statSync(path, { bigint: true });
        const edited = original.replace("e4", "e3");
        writeFileSync(path, edited);
        utimesSync(path, 1_700_000_000, 1_700_000_000);
        const after = statSync(path, { bigint: true });
        expect([after.size, after.mtimeNs]).toEqual([
          before.size,
          before.mtimeNs,
        ]);
        if (change === "before") await remove(session, 2);
        await expectRefusal(
          session,
          "This file changed since the list was read. Reopen the file and choose the game again.",
        );
        expect(bytes()).toBe(edited);
        await checkHistory(session, []);
      });
    });
  }

  it("detaches the deleted board and renumbers the open board for its identical neighbour", async () => {
    const pair = `${bea}\n${bea}`;
    const appended = `${bea}\n[White "Bea"]\n[Black "Bo"]\n\n1. d4 d5 *\n`;
    await openClub(pair, async (session) => {
      const openGame = (number: number) =>
        session
          .$(`main table button[aria-label="Open game ${number}"]`)
          .click();
      const tabs = () => session.$$('header [role="tab"]');
      const notation = () => session.$$("main aside ol button");
      const expectCurrentMove = (move: string) =>
        expect(session.$("main aside ol").$(`button=${move}`)).toHaveAttribute(
          "aria-current",
          "true",
        );
      await openGame(1);
      await session.$("main aside ol").$("button=d4").click();
      await expectCurrentMove("d4");
      const board = session.$("main cg-board");
      const side = (await board.getSize("width")) / 8;
      for (const rank of [7, 5])
        await board.click({ x: -0.5 * side, y: (4.5 - rank) * side });
      await expectCurrentMove("d5");
      await files(session);
      await openGame(2);
      await expect(tabs()).toBeElementsArrayOfSize(2);
      await files(session);
      await remove(session, 1);
      await confirm(session);
      await checkRows(session, ["Bea"]);
      expect(bytes()).toBe(bea);
      await openGame(1);
      await expect(tabs()).toBeElementsArrayOfSize(2);
      await expect(tabs()[1]).toHaveAttribute("aria-selected", "true");
      expect(await notation().map((move) => move.getText())).toEqual(["d4"]);

      await tabs()[0]?.click();
      await expectCurrentMove("d5");
      expect(await notation().map((move) => move.getText())).toEqual([
        "d4",
        "d5",
      ]);
      await session.$("main").$("aria/Set up position").click();
      await expect(session.$("main").$("aria/FEN")).toHaveValue(
        "rnbqkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq - 0 2",
      );
      await session.$("main").$("button=Cancel").click();
      await session.$("main aside").$("aria/Save as new game").click();
      await session.$("main aside form").$("aria/Save").click();
      await expect(
        session.$('header [aria-selected="true"] span:empty'),
      ).not.toExist();
      expect(bytes()).toBe(appended);
      await files(session);
      await openGame(2);
      await expect(tabs()).toBeElementsArrayOfSize(2);
      await expect(tabs()[0]).toHaveAttribute("aria-selected", "true");
      await files(session);
      await checkHistory(session, [2, 1]);
      await restore(session, 1);
      await session.waitUntil(() => bytes() === pair);
      expect(bytes()).toBe(pair);
      await restore(session, 2);
      await checkRows(session, ["Bea"]);
      expect(bytes()).toBe(bea);
    });
  });

  it("keeps both games when their boundary cannot be located safely", async () => {
    const ambiguous = '[White "A"]\n\n[White "B"]\n\n1. e4 *\n';
    await openClub(ambiguous, async (session) => {
      await checkRows(session, ["A", "B"]);
      await remove(session, 1);
      await expectRefusal(
        session,
        `Could not locate game 1 safely in ${path}. Your PGN file is untouched.`,
      );
      expect(bytes()).toBe(ambiguous);
      await checkRows(session, ["A", "B"]);
      await checkHistory(session, []);
    });
  });
});
