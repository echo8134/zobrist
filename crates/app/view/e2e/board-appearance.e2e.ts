import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { expect } from "@wdio/globals";
import { target, withApp } from "./app-window";
import { drawnBoard, startPositionWithWhiteAtBottom } from "./board";

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

async function expectCoordinates(
  app: WebdriverIO.Browser,
  files: string,
  ranks: string,
) {
  await app.waitUntil(
    () =>
      app.execute<boolean, [string, string]>(
        `
        const labels = (kind) => [...document.querySelectorAll("main coords." + kind + " coord")]
          .filter(coord => coord.getClientRects().length).map(coord => coord.textContent).sort().join("");
        return labels("files") === arguments[0] && labels("ranks") === arguments[1];
      `,
        files,
        ranks,
      ),
    { timeoutMsg: `Expected board coordinates "${files}" and "${ranks}"` },
  );
}

async function expectPiece(
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
  await expectPiece(
    app,
    "cg-board piece.white.knight",
    "/pieces/chessnut/wN.svg",
  );
  await app.waitUntil(
    async () => {
      const { value } = await app
        .$("cg-board")
        .getCSSProperty("background-image");
      return (
        typeof value === "string" &&
        value.includes("rgb(238, 238, 210)") &&
        value.includes("rgb(118, 150, 86)")
      );
    },
    { timeoutMsg: "Expected green board squares #eeeed2 and #769656" },
  );
}

async function expectDefaultBoard(app: WebdriverIO.Browser) {
  const board = app.$("main cg-board");
  await expect(board).toBeDisplayed();
  await expectPiece(
    app,
    "cg-board piece.white.knight",
    "/pieces/cburnett/wN.svg",
  );
  const colour = await board.getCSSProperty("background-color");
  expect(colour.parsed.hex).toBe("#f0d9b5");
  await expectCoordinates(app, "abcdefgh", "12345678");
}

describe("board appearance", () => {
  const { ZOBRIST_TEST_DATA: runDirectory } = process.env;
  if (!runDirectory)
    throw new Error("The window test needs its isolated data folder");

  for (const set of [
    {
      name: "cburnett",
      credit: "Cburnett chess pieces by Colin M.L. Burnett",
      license: "GPL-2.0-or-later",
      heading: "GNU GENERAL PUBLIC LICENSE",
      licenseDigest:
        "00d857bed08903a4f02325061f98ded6340800f1a18706e0473f656ecb884bbe",
    },
    {
      name: "chessnut",
      credit: "Chessnut chess pieces by Alexis Luengas",
      license: "Apache-2.0",
      heading: "Apache License",
      licenseDigest:
        "0ffddef9e48f8a09aed5caf2d44f7ba1c1be2d9b8e0a6f693b1635b2d5566645",
    },
  ]) {
    it(`bundles every ${set.name} piece, its credit and the full ${set.license} text`, () => {
      const folder = new URL(`../dist/pieces/${set.name}/`, import.meta.url);
      const images = readdirSync(folder, { withFileTypes: true })
        .filter((entry) => entry.isFile() && entry.name.endsWith(".svg"))
        .map((entry) => entry.name)
        .sort();
      expect(images).toEqual([
        "bB.svg",
        "bK.svg",
        "bN.svg",
        "bP.svg",
        "bQ.svg",
        "bR.svg",
        "wB.svg",
        "wK.svg",
        "wN.svg",
        "wP.svg",
        "wQ.svg",
        "wR.svg",
      ]);
      const license = readFileSync(new URL("LICENSE.txt", folder), "utf8");
      expect(license.split(/\r?\n/).slice(0, 2)).toEqual([
        set.credit,
        set.license,
      ]);
      // Pin the published license text independently of line wrapping and the source links above it.
      const words = license.trim().replace(/\s+/g, " ");
      const start = words.indexOf(set.heading);
      expect(start).toBeGreaterThan(0);
      expect(
        createHash("sha256").update(words.slice(start)).digest("hex"),
      ).toBe(set.licenseDigest);
    });
  }

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
      await expect(
        control(app, "Piece set").$("option:checked"),
      ).toHaveElementProperty("label", "Cburnett");
      await expect(
        control(app, "Board colours").$("option:checked"),
      ).toHaveElementProperty("label", "Brown");
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
      await expectCoordinates(app, "", "");
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
      await expectCoordinates(app, "", "");
    });

    await withApp(data, async (app) => {
      await openBoard(app, 1);
      await expectChessnutBoard(app);
      await expectCoordinates(app, "", "");
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
      await expectCoordinates(app, "abcdefgh", "12345678");
      await app.$("main").$("aria/Set up position").click();
      await expectPiece(
        app,
        'button[aria-label="White knight"] piece',
        "/pieces/chessnut/wN.svg",
      );
      await control(app, "FEN").setValue("7k/P7/8/8/8/8/8/4K3 w - - 0 1");
      await expect(app.$$("cg-board piece")).toBeElementsArrayOfSize(3);
      await app.$("main").$("aria/Use position").click();
      await expect(app.$("main").$("aria/Set up position")).toBeEnabled();
      const board = app.$("main cg-board");
      const square = (await board.getSize("width")) / 8;
      await board.click({ x: -3.5 * square, y: -2.5 * square });
      await board.click({ x: -3.5 * square, y: -3.5 * square });
      for (const [role, file] of Object.entries({
        Queen: "wQ.svg",
        Knight: "wN.svg",
        Rook: "wR.svg",
        Bishop: "wB.svg",
      })) {
        await expectPiece(
          app,
          `button[aria-label="${role}"] piece`,
          `/pieces/chessnut/${file}`,
        );
      }
    });
  });

  for (const [problem, contents] of Object.entries({
    "malformed JSON": "{",
    "an unknown piece set":
      '{"pieceSet":"marble","boardPalette":"green","showCoordinates":false}',
    "an unknown palette":
      '{"pieceSet":"chessnut","boardPalette":"blue","showCoordinates":false}',
  })) {
    it(`shows the default board and explains how to recover settings with ${problem}`, async () => {
      const data = `${runDirectory}/damaged-board-settings`;
      const directory = `${data}/io.github.echo8134.zobrist`;
      const path = `${directory}/settings.json`;
      mkdirSync(directory, { recursive: true });
      writeFileSync(path, contents);
      await withApp(data, async (app) => {
        await openBoard(app, 1);
        await expectDefaultBoard(app);
        await settings(app).click();
        await expect(app.$("main [role=alert]")).toHaveText(
          `Could not read the board settings at ${path}. Restore the file from a backup or move it aside, then restart zobrist.`,
        );
      });
      expect(readFileSync(path, "utf8")).toBe(contents);
    });
  }
});
