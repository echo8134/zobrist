import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { open } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { visitPage } from "./desktop-session";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const capturedAt = "2026-01-02 09:30:00 UTC";
const game = (white: string) => `[White "${white}"]\n\n*\n`;
const contents = (path: string) => readFileSync(path, "utf8");
const history = (session: WebdriverIO.Browser) =>
  session.$('section[aria-label="Earlier states"]');
const state = (session: WebdriverIO.Browser, number: number) =>
  history(session).$(`aria/State ${number}`);
const dialog = (session: WebdriverIO.Browser) => session.$("dialog[open]");

async function selectFile(
  session: WebdriverIO.Browser,
  path: string,
  choice: "New" | "File" = "File",
) {
  await pick(choice, path, session);
  await expect(
    session.$('section[aria-label="Selection"]').$(`p=${path}`),
  ).toBeDisplayed();
}

async function showHistory(session: WebdriverIO.Browser) {
  await session.$("nav").$("aria/Files").click();
  const toggle = session.$("main").$("button=Earlier states");
  if ((await toggle.getAttribute("aria-expanded")) !== "true")
    await toggle.click();
  await expect(history(session)).toBeDisplayed();
}

async function checkStateNumbers(
  session: WebdriverIO.Browser,
  expected: number[],
) {
  await expect(
    history(session).$(
      `li:first-child button[aria-label="State ${expected[0]}"]`,
    ),
  ).toExist();
  const buttons = history(session).$$("li button");
  await expect(buttons).toBeElementsArrayOfSize(expected.length);
  expect(
    await buttons.map((button) => button.getAttribute("aria-label")),
  ).toEqual(expected.map((number) => `State ${number}`));
}

function firstHistoryFolder(work: string) {
  const root = join(work, "data", "io.github.echo8134.zobrist", "pgn-history");
  const folders = readdirSync(root, { withFileTypes: true }).filter((entry) =>
    entry.isDirectory(),
  );
  const [first] = folders;
  if (folders.length !== 1 || !first)
    throw Error(`Expected one PGN history in ${root}`);
  return join(root, first.name);
}

async function editWhite(session: WebdriverIO.Browser, name: string) {
  const form = session.$("main aside form");
  if (!(await form.isDisplayed()))
    await session.$("main aside").$("aria/Show the game data").click();
  await form.$("aria/White").setValue(name);
  await session.keys("Enter");
  await expect(session.$('header [aria-selected="true"] span:empty')).toExist();
}

async function appendGame(session: WebdriverIO.Browser, name: string) {
  await session.$("header").$("aria/New board").click();
  await editWhite(session, name);
  await session.keys(["Control", "s"]);
  await session.$("main aside form").$("aria/Save").click();
  await expect(
    session.$('header [aria-selected="true"] span:empty'),
  ).not.toExist();
}

async function replaceGame(session: WebdriverIO.Browser, name: string) {
  await editWhite(session, name);
  await session.keys(["Control", "s"]);
  await expect(
    session.$('header [aria-selected="true"] span:empty'),
  ).not.toExist();
}

async function restore(session: WebdriverIO.Browser, number: number) {
  await state(session, number).click();
  await dialog(session).$("aria/Restore").click();
}

describe("earlier PGN states", function () {
  this.timeout(120_000);
  let work: string;
  beforeEach(() => {
    work = mkdtempSync(join(tmpdir(), "zobrist-history-"));
  });
  afterEach(() => {
    rmSync(work, { recursive: true, force: true });
  });

  it("keeps an annotated file across a restart, restores it and undoes the restore", async () => {
    const path = join(work, "Club.pgn");
    const original = '[Event "Club"]\r\n\r\n1. e4 {home} *\r\n';
    const damaged = '[Event "unfinished';
    writeFileSync(path, original);
    await visitPage(
      work,
      "Files",
      async (session) => {
        await selectFile(session, path);
        await showHistory(session);
        await expect(
          history(session).$("p=No earlier states."),
        ).toBeDisplayed();
        await appendGame(session, "Second");
        await showHistory(session);
        await checkStateNumbers(session, [1]);
        await expect(history(session).$(`span=${capturedAt}`)).toBeDisplayed();
        await expect(history(session).$("span=34 bytes")).toBeDisplayed();
      },
      { ZOBRIST_TEST_HISTORY_TIME: "1767346200" },
    );

    writeFileSync(path, damaged);
    await visitPage(work, "Files", async (session) => {
      await session.$("main").$("button=Club.pgn").click();
      await showHistory(session);
      await checkStateNumbers(session, [1]);
      await expect(history(session).$(`span=${capturedAt}`)).toBeDisplayed();
      await expect(history(session).$("span=34 bytes")).toBeDisplayed();
      await state(session, 1).click();
      await expect(dialog(session)).toHaveText(
        expect.stringContaining("State 1"),
      );
      await expect(dialog(session)).toHaveText(expect.stringContaining(path));
      mkdirSync(screenshots, { recursive: true });
      const picture = await session.saveScreenshot(
        join(screenshots, "files-earlier-states.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
      await dialog(session).$("aria/Cancel").click();
      await expect(dialog(session)).not.toExist();
      expect(contents(path)).toBe(damaged);
      await checkStateNumbers(session, [1]);
      await state(session, 1).click();
      await expect(dialog(session)).toBeDisplayed();
      writeFileSync(path, "1. d4 *\n");
      await dialog(session).$("aria/Restore").click();
      await expect(history(session).$('[role="alert"]')).toHaveText(
        "This file changed. Choose the state again.",
      );
      expect(contents(path)).toBe("1. d4 *\n");
      await checkStateNumbers(session, [1]);
      writeFileSync(path, damaged);
      await restore(session, 1);
      await expect(
        session.$("main table tbody tr").$("td=Club"),
      ).toBeDisplayed();
      expect(contents(path)).toBe(original);
      await checkStateNumbers(session, [2, 1]);
      await expect(
        history(session).$("li:first-child").$("span=18 bytes"),
      ).toBeDisplayed();
      await expect(
        history(session).$("li:last-child").$("span=34 bytes"),
      ).toBeDisplayed();
      await restore(session, 2);
      await session.waitUntil(() => contents(path) === damaged);
      expect(contents(path)).toBe(damaged);
    });
  });

  it("keeps twenty changes from a board and refuses a stale save after restoring the oldest", async () => {
    const path = join(work, "Series.pgn");
    const twenty = [
      21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2,
    ];
    const afterRestore = [
      22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3,
    ];
    await visitPage(work, "Files", async (session) => {
      await selectFile(session, path, "New");
      await appendGame(session, "01");
      await showHistory(session);
      await checkStateNumbers(session, [1]);
      await expect(history(session).$("span=0 bytes")).toBeDisplayed();
      const tab = session.$('header [role="tab"]');
      await tab.click();
      for (let number = 2; number <= 21; number++) {
        await replaceGame(session, String(number).padStart(2, "0"));
      }
      await showHistory(session);
      await checkStateNumbers(session, twenty);
      await tab.click();
      await session.keys(["Control", "s"]);
      await showHistory(session);
      await checkStateNumbers(session, twenty);
      await restore(session, 2);
      await checkStateNumbers(session, afterRestore);
      expect(contents(path)).toBe(game("01"));
      await tab.click();
      await editWhite(session, "22");
      await session.keys(["Control", "s"]);
      await expect(session.$('main aside [role="alert"]')).toHaveText(
        "The file changed since this game was opened. Save it as a new game.",
      );
      expect(contents(path)).toBe(game("01"));
      await showHistory(session);
      await checkStateNumbers(session, afterRestore);
      await restore(session, 22);
      await session.waitUntil(() => contents(path) === game("21"));
      expect(contents(path)).toBe(game("21"));
    });
  });

  it("refuses a busy save, keeps the edit and lets the player save it again", async () => {
    const path = join(work, "Busy.pgn");
    const other = join(work, "Other.pgn");
    await visitPage(work, "Files", async (session) => {
      await selectFile(session, path, "New");
      await appendGame(session, "Ada");
      await editWhite(session, "Bea");

      const folder = firstHistoryFolder(work);
      const owner = await open(join(folder, "owner"), "r");
      const watchdog = setTimeout(() => void owner.close(), 15_000);
      try {
        // flock locks the open file description shared with the test's handle.
        execFileSync("flock", ["--exclusive", "--nonblock", "3"], {
          stdio: ["ignore", "ignore", "pipe", owner.fd],
        });
        const started = performance.now();
        await session.keys(["Control", "s"]);
        await expect(session.$('main aside [role="alert"]')).toHaveText(
          "Another change to this file is still running. Try again.",
          { wait: 8_000 },
        );
        expect(performance.now() - started).toBeGreaterThanOrEqual(5_000);
        expect(owner.fd).not.toBe(-1);
        expect(contents(path)).toBe(game("Ada"));
        expect(existsSync(join(folder, "2.state"))).toBe(false);
        await expect(
          session.$('header [aria-selected="true"] span:empty'),
        ).toExist();

        await session.$("nav").$("aria/Files").click();
        await selectFile(session, other, "New");
        await appendGame(session, "Cy");
        expect(contents(other)).toBe(game("Cy"));
        expect(owner.fd).not.toBe(-1);
      } finally {
        clearTimeout(watchdog);
        await owner.close();
      }

      await session.$('header [role="tab"]').click();
      await session.keys(["Control", "s"]);
      await expect(
        session.$('header [aria-selected="true"] span:empty'),
      ).not.toExist();
      expect(contents(path)).toBe(game("Bea"));
      await session.$("nav").$("aria/Files").click();
      await selectFile(session, path);
      await showHistory(session);
      await checkStateNumbers(session, [2, 1]);
      await restore(session, 2);
      await session.waitUntil(() => contents(path) === game("Ada"));
    });
  });

  it("saves with a warning but refuses to restore when the history folder is read-only", async () => {
    const path = join(work, "Final.pgn");
    await visitPage(work, "Files", async (session) => {
      await selectFile(session, path, "New");
      await appendGame(session, "Ada");
      const folder = firstHistoryFolder(work);
      const first = join(folder, "1.state");
      const retained = readFileSync(first);
      chmodSync(folder, 0o555);
      try {
        await replaceGame(session, "Bea");
        expect(contents(path)).toBe(game("Bea"));
        await expect(session.$('main aside [role="alert"]')).toHaveText(
          `Game saved. Could not keep the earlier state in ${folder}. The history folder is read-only.`,
        );
        await showHistory(session);
        await checkStateNumbers(session, [1]);
        await expect(history(session).$("span=0 bytes")).toBeDisplayed();
        await restore(session, 1);
        await expect(history(session).$('[role="alert"]')).toHaveText(
          `Could not restore State 1. The current bytes could not be kept in ${folder}. The history folder is read-only. Allow writing to this folder and try again.`,
        );
        expect(contents(path)).toBe(game("Bea"));
        expect(readFileSync(first)).toEqual(retained);
      } finally {
        chmodSync(folder, 0o755);
      }
    });
  });

  it("keeps other states available when one file of one history is damaged", async () => {
    const club = join(work, "Club", "Final.pgn");
    const league = join(work, "League", "Final.pgn");
    mkdirSync(join(work, "Club"));
    mkdirSync(join(work, "League"));
    await visitPage(work, "Files", async (session) => {
      await selectFile(session, club, "New");
      await appendGame(session, "Ada");
      await replaceGame(session, "Bea");
      const damaged = join(firstHistoryFolder(work), "2.state");
      await session.$("nav").$("aria/Files").click();
      await selectFile(session, league, "New");
      await appendGame(session, "Bo");
      writeFileSync(damaged, "[[");
      await session.$("nav").$("aria/Files").click();
      await selectFile(session, club);
      await showHistory(session);
      await state(session, 2).click();
      await expect(history(session).$('[role="alert"]')).toHaveText(
        `Could not read ${damaged}. Your PGN files are untouched. Restore this state file from a backup or choose another state.`,
      );
      expect(contents(club)).toBe(game("Bea"));
      expect(contents(damaged)).toBe("[[");
      await session.$('header [role="tab"]').click();
      await replaceGame(session, "Cara");
      expect(contents(club)).toBe(game("Cara"));
      await showHistory(session);
      await checkStateNumbers(session, [3, 2, 1]);
      await restore(session, 3);
      await session.waitUntil(() => contents(club) === game("Bea"));
      await restore(session, 1);
      await session.waitUntil(() => contents(club) === "");
      await selectFile(session, league);
      await showHistory(session);
      await checkStateNumbers(session, [1]);
      await restore(session, 1);
      await session.waitUntil(() => contents(league) === "");
      expect([contents(club), contents(league), contents(damaged)]).toEqual([
        "",
        "",
        "[[",
      ]);
    });
  });

  it("finds earlier states again only when a moved PGN returns to its path", async () => {
    const club = join(work, "Club", "Final.pgn");
    const league = join(work, "League", "Final.pgn");
    mkdirSync(join(work, "Club"));
    mkdirSync(join(work, "League"));
    await visitPage(work, "Files", async (session) => {
      await selectFile(session, club, "New");
      await appendGame(session, "Ada");
      const first = join(firstHistoryFolder(work), "1.state");
      const retained = readFileSync(first);
      renameSync(club, league);
      await session.$("nav").$("aria/Files").click();
      await selectFile(session, league);
      await showHistory(session);
      await expect(history(session).$("p=No earlier states.")).toBeDisplayed();
      await session
        .$('section[aria-label="Collection"]')
        .$(`button[title="${club}"]`)
        .click();
      await expect(
        session.$('section[aria-label="Selection"] [role="status"]'),
      ).toHaveText(
        "This file cannot be opened. No such file or directory (os error 2)",
      );
      expect(existsSync(club)).toBe(false);
      renameSync(league, club);
      await selectFile(session, club);
      await showHistory(session);
      await checkStateNumbers(session, [1]);
      rmSync(club);
      await session.$("nav").$("aria/Home").click();
      await session.$("nav").$("aria/Files").click();
      expect(existsSync(club)).toBe(false);
      expect(readFileSync(first)).toEqual(retained);
    });
  });
});
