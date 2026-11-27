import { once } from "node:events";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { cleanupWdioSession, startWdioSession } from "@wdio/tauri-service";
import { drawnBoard, startPositionWithWhiteAtBottom } from "./board";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));

async function availablePort() {
  // The normal test window already owns its driver's port.
  await using listener = createServer().listen(0, "127.0.0.1");
  await once(listener, "listening");
  const address = listener.address();
  if (!address || typeof address === "string") throw new Error("No TCP port");
  return address.port;
}

async function withApp(
  data: string,
  check: (app: WebdriverIO.Browser) => Promise<void>,
) {
  const app = await startWdioSession({
    "tauri:options": { application: `${target}/debug/zobrist-app` },
    "wdio:tauriServiceOptions": {
      embeddedPort: await availablePort(),
      env: { XDG_DATA_HOME: data, XDG_CACHE_HOME: `${data}/cache` },
    },
  });
  try {
    await app.switchToWindow("main");
    await check(app);
  } finally {
    await cleanupWdioSession(app);
  }
}

const settings = (app: WebdriverIO.Browser) => app.$("nav").$("aria/Settings");
async function openBoard(app: WebdriverIO.Browser, tabCount: number) {
  await app.$("header").$("aria/New board").click();
  await expect(app.$$("header [role=tab]")).toBeElementsArrayOfSize(tabCount);
}
const shownTab = (app: WebdriverIO.Browser) => app.$("header [role=tab]");
const control = (app: WebdriverIO.Browser, label: string) =>
  app.$(
    `//main//label[normalize-space(text())="${label}"]/*[self::select or self::input]`,
  );

// The native select menu is outside the web view.
async function choose(app: WebdriverIO.Browser, label: string, text: string) {
  const select = await control(app, label).getElement();
  await expect(select).toBeEnabled();
  await app.execute(
    `
      const select = arguments[0];
      select.value = [...select.options].find(option => option.text === arguments[1]).value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
    `,
    select,
    text,
  );
}

async function coordinates(app: WebdriverIO.Browser) {
  return app.execute(`
    const labels = (kind) => [...document.querySelectorAll("main coords." + kind + " coord")]
      .filter(coord => coord.getClientRects().length).map(coord => coord.textContent).sort().join("");
    return [labels("files"), labels("ranks")];
  `);
}

async function expectKnight(
  app: WebdriverIO.Browser,
  selector: string,
  image: string,
) {
  // Changing boards can replace the piece element while the window test is reading it.
  await app.waitUntil(
    () =>
      app.execute<boolean, [string, string]>(
        `
          const piece = document.querySelector(arguments[0]);
          return !!piece && piece.checkVisibility() && piece.getBoundingClientRect().width > 0
            && getComputedStyle(piece).backgroundImage.includes(arguments[1]);
        `,
        selector,
        image,
      ),
    { timeoutMsg: `Expected a visible ${selector} using ${image}` },
  );
}

async function expectChessnutBoard(app: WebdriverIO.Browser) {
  await expectKnight(
    app,
    "cg-board piece.white.knight",
    "/pieces/chessnut/wN.svg",
  );
  const palette = await app.$("cg-board").getCSSProperty("background-image");
  expect(palette.value).toContain("rgb(238, 238, 210)");
  expect(palette.value).toContain("rgb(118, 150, 86)");
}

async function expectDefaultBoard(app: WebdriverIO.Browser) {
  const board = app.$("main cg-board");
  await expect(board).toBeDisplayed();
  await expectKnight(
    app,
    "cg-board piece.white.knight",
    "/pieces/cburnett/wN.svg",
  );
  const colour = await board.getCSSProperty("background-color");
  expect(colour.parsed.hex).toBe("#f0d9b5");
  expect(await coordinates(app)).toEqual(["abcdefgh", "12345678"]);
}

describe("board appearance", () => {
  const { ZOBRIST_TEST_DATA: runDirectory } = process.env;
  if (!runDirectory)
    throw new Error("The window test needs its isolated data folder");

  it("uses the choices on open and new boards and after quitting the app", async function () {
    this.timeout(60000);
    const data = `${runDirectory}/board-appearance`;
    await withApp(data, async (app) => {
      await settings(app).click();
      await expect(control(app, "Piece set")).toHaveValue("cburnett");
      for (const label of ["Piece set", "Board colours", "Show coordinates"]) {
        expect(await control(app, label).getComputedLabel()).toBe(label);
      }
      await expect(control(app, "Board colours")).toHaveValue("brown");
      await expect(control(app, "Show coordinates")).toBeChecked();
      await openBoard(app, 1);
      await expectDefaultBoard(app);
      const board = app.$("main cg-board");
      const square = (await board.getSize("width")) / 8;
      await board.click({ x: 0.5 * square, y: 2.5 * square });
      await board.click({ x: 0.5 * square, y: 0.5 * square });
      await expect(app.$("main aside").$("aria/e4")).toBeDisplayed();

      await settings(app).click();
      await choose(app, "Piece set", "Chessnut");
      await choose(app, "Board colours", "Green");
      await control(app, "Show coordinates").click();
      await shownTab(app).click();
      await expectChessnutBoard(app);
      expect(await coordinates(app)).toEqual(["", ""]);
      expect(await drawnBoard(app)).toEqual([
        "rnbqkbnr",
        "pppppppp",
        "........",
        "........",
        "....P...",
        "........",
        "PPPP.PPP",
        "RNBQKBNR",
      ]);
      await openBoard(app, 2);
      await expectChessnutBoard(app);
      expect(await drawnBoard(app)).toEqual(startPositionWithWhiteAtBottom);
      expect(await coordinates(app)).toEqual(["", ""]);
    });

    await withApp(data, async (app) => {
      await openBoard(app, 1);
      await expectChessnutBoard(app);
      expect(await coordinates(app)).toEqual(["", ""]);
      const image = await app.saveScreenshot(
        `${target}/e2e/board-appearance.png`,
      );
      expect([image.readUInt32BE(16), image.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
      await settings(app).click();
      await expect(control(app, "Piece set")).toHaveValue("chessnut");
      await expect(control(app, "Board colours")).toHaveValue("green");
      await expect(control(app, "Show coordinates")).not.toBeChecked();
      await control(app, "Show coordinates").click();
      await shownTab(app).click();
      await expectChessnutBoard(app);
      expect(await coordinates(app)).toEqual(["abcdefgh", "12345678"]);
      await app.$("main").$("aria/Set up position").click();
      await expectKnight(
        app,
        'button[aria-label="White knight"] piece',
        "/pieces/chessnut/wN.svg",
      );
    });
  });

  it("shows the default board and explains how to recover the settings file", async () => {
    const data = `${runDirectory}/damaged-board-settings`;
    const directory = `${data}/io.github.echo8134.zobrist`;
    const path = `${directory}/settings.json`;
    mkdirSync(directory, { recursive: true });
    writeFileSync(path, "{");
    await withApp(data, async (app) => {
      await openBoard(app, 1);
      await expectDefaultBoard(app);
      await settings(app).click();
      await expect(app.$("main [role=alert]")).toHaveText(
        `Could not read the board settings at ${path}. Restore the file from a backup or move it aside, then restart zobrist.`,
      );
    });
    expect(readFileSync(path, "utf8")).toBe("{");
  });
});
