import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { drawnBoard, startPositionWithWhiteAtBottom } from "./board";
import { withDesktopApp } from "./desktop-session";
import { pick } from "./file-picker";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));
const fixture = fileURLToPath(
  new URL("../../../cli/tests/fixtures/clean.pgn", import.meta.url),
);

const tabs = (app: WebdriverIO.Browser) => app.$$('header [role="tab"]');
const selected = (app: WebdriverIO.Browser) =>
  app.$('header [role="tab"][aria-selected="true"]');
const expectMove = (app: WebdriverIO.Browser, move: string) =>
  expect(app.$("main aside ol").$(`button=${move}`)).toHaveAttribute(
    "aria-current",
    "true",
  );
const comment = (app: WebdriverIO.Browser) =>
  app.$("main aside").$("aria/Comment");
const newBoard = (app: WebdriverIO.Browser) =>
  app.$("header").$("aria/New board").click();

async function expectHome(app: WebdriverIO.Browser) {
  await expect(app.$("main h1")).toHaveText("Home");
  await expect(tabs(app)).toBeElementsArrayOfSize(0);
}

async function openGameTwo(
  app: WebdriverIO.Browser,
  file: string,
  openGameOne: boolean,
) {
  const files = () => app.$("nav").$("aria/Files").click();
  await files();
  await pick("File", file, app);
  if (openGameOne) {
    await app.$("main table").$("td=Alpha").click();
    await expect(selected(app)).toHaveText("Alpha - Beta");
    await files();
  }
  await app.$("main table").$("td=Gamma").click();
  await app.$("main aside").$("button=Qh4#").click();
  await expectMove(app, "Qh4#");
}

async function addComment(app: WebdriverIO.Browser) {
  await comment(app).setValue("Final move");
  await app.$("main aside").$("button=Qh4#").click();
  await expect(selected(app)).toHaveAttribute(
    "aria-label",
    "Gamma - Delta, not saved",
  );
}

async function play(
  app: WebdriverIO.Browser,
  from: string,
  to: string,
  move: string,
) {
  const board = app.$("main cg-board");
  const side = (await board.getSize("width")) / 8;
  for (const square of [from, to]) {
    await board.click({
      x: ("abcdefgh".indexOf(square.charAt(0)) - 3.5) * side,
      y: (4.5 - Number(square.charAt(1))) * side,
    });
  }
  await expectMove(app, move);
}

describe("boards after quitting and starting zobrist", function () {
  this.timeout(120_000);
  let work: string;
  let file: string;
  beforeEach(() => {
    mkdirSync(`${target}/e2e`, { recursive: true });
    work = mkdtempSync(`${target}/e2e/boards-`);
    file = `${work}/clean.pgn`;
    copyFileSync(fixture, file);
  });
  afterEach(() => rmSync(work, { recursive: true, force: true }));

  it("opens Home or the readable board and tells the player where the original session is kept", async () => {
    const directory = `${work}/io.github.echo8134.zobrist`;
    mkdirSync(directory);
    const session = `${directory}/boards.json`;
    const truncated = '{"boards":[';
    writeFileSync(session, truncated);
    const firstBackup = `${directory}/boards-recovery-1.json`;
    await withDesktopApp(work, async (app) => {
      await expectHome(app);
      await expect(app.$('[role="alert"]')).toHaveText(
        `Could not read the board session. The original file was kept at ${firstBackup}. Your PGN files are untouched. You can open a board and keep playing.`,
      );
      await newBoard(app);
      await expect(selected(app)).toHaveText("New game");
    });
    expect(readFileSync(firstBackup, "utf8")).toBe(truncated);

    const mixed = JSON.stringify({
      boards: [
        { game: "1. e4 e5 2. e5 *\n", place: [], origin: null },
        { game: "1. d4 *\n", place: [0], origin: null },
        { game: "*\n", place: [0], origin: null },
      ],
      selected: 1,
    });
    writeFileSync(session, mixed);
    const secondBackup = `${directory}/boards-recovery-2.json`;
    await withDesktopApp(work, async (app) => {
      await expect(tabs(app)).toBeElementsArrayOfSize(1);
      await expectMove(app, "d4");
      await expect(app.$('[role="alert"]')).toHaveText(
        `2 boards could not be reopened. The original file was kept at ${secondBackup}. Your PGN files are untouched. You can open a board and keep playing.`,
      );
    });
    expect(readFileSync(firstBackup, "utf8")).toBe(truncated);
    expect(readFileSync(secondBackup, "utf8")).toBe(mixed);
  });

  it("restores all three boards, saves game two in place and remembers the last closed tab", async () => {
    await withDesktopApp(work, async (app) => {
      await expectHome(app);
      await openGameTwo(app, file, true);
      await addComment(app);
      await newBoard(app);
      await play(app, "d2", "d4", "d4");
      await play(app, "d7", "d5", "d5");
      await app.keys("ArrowLeft");
      await expectMove(app, "d4");
      await play(app, "g8", "f6", "Nf6");
    });

    const original = readFileSync(fixture);
    await withDesktopApp(work, async (app) => {
      await expect(tabs(app)).toBeElementsArrayOfSize(3);
      expect(await tabs(app).map((tab) => tab.getText())).toEqual([
        "Alpha - Beta",
        "Gamma - Delta",
        "New game",
      ]);
      expect(await tabs(app).map((tab) => tab.$$("span:empty").length)).toEqual(
        [0, 1, 1],
      );
      await expect(selected(app)).toHaveText("New game");
      await expect(
        app.$('main aside ol ol button[aria-current="true"]'),
      ).toHaveText("Nf6");
      expect(
        await app
          .$$("main aside div > ol > li > span > button")
          .map((button) => button.getText()),
      ).toEqual(["d4", "d5"]);
      expect(readFileSync(file)).toEqual(original);
      const picture = await app.saveScreenshot(
        `${target}/e2e/restored-boards.png`,
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);

      await app.$("header").$("button=Gamma - Delta").click();
      await expectMove(app, "Qh4#");
      await expect(comment(app)).toHaveValue("Final move");
      await app.$("main aside").$("aria/Save").click();
      await expect(selected(app).$("span:empty")).not.toExist();
      expect(readFileSync(file)).toEqual(
        Buffer.from(
          original.toString().replace("Qh4# 0-1", "Qh4# {Final move} 0-1"),
        ),
      );
      await app.$("nav").$("aria/Files").click();
      await app.$("main").$("button=clean.pgn").click();
      await app.$("main table").$("td=Gamma").click();
      await expect(selected(app)).toHaveText("Gamma - Delta");
      await expect(tabs(app)).toBeElementsArrayOfSize(3);

      const header = app.$("header");
      await header.$("aria/Close New game").click();
      await header.$("button=Close").click();
      await header.$("aria/Close Gamma - Delta").click();
      await header.$("aria/Close Alpha - Beta").click();
      await expect(tabs(app)).toBeElementsArrayOfSize(0);
    });

    await withDesktopApp(work, expectHome);
    await withDesktopApp(work, async (app) => {
      await expectHome(app);
      await newBoard(app);
      await expect(selected(app)).toHaveText("New game");
    });
    await withDesktopApp(work, async (app) => {
      await expect(tabs(app)).toBeElementsArrayOfSize(1);
      await expect(selected(app)).toHaveText("New game");
      await expect(selected(app).$("span:empty")).not.toExist();
      expect(await drawnBoard(app)).toEqual(startPositionWithWhiteAtBottom);
    });
  });

  it("keeps the unsaved comment and an outside edit to game two", async () => {
    await withDesktopApp(work, async (app) => {
      await expectHome(app);
      await openGameTwo(app, file, false);
      const blocked = `${work}/io.github.echo8134.zobrist/boards.json`;
      mkdirSync(blocked);
      expect(await app.closeWindow()).toEqual(["main"]);
      await expect(app.$('[role="alert"]')).toHaveText(
        expect.stringContaining("Cannot save your boards before quitting."),
      );
      await addComment(app);
      rmdirSync(blocked);
    });
    const outside = readFileSync(fixture, "utf8").replace(
      '[White "Gamma"]',
      '[White "Outside"]',
    );
    writeFileSync(file, outside);
    await withDesktopApp(work, async (app) => {
      await expect(selected(app)).toHaveText("Gamma - Delta");
      await app.$("main aside").$("aria/Save").click();
      await expect(app.$('main aside [role="alert"]')).toHaveText(
        "The file changed since this game was opened. Save it as a new game.",
      );
      expect(readFileSync(file, "utf8")).toBe(outside);
      await expectMove(app, "Qh4#");
      await expect(comment(app)).toHaveValue("Final move");
      await expect(selected(app).$("span:empty")).toExist();
    });
  });
});
