import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";
import { drawnBoard, startPositionWithWhiteAtBottom } from "./board";
import { nativeKey } from "./native-key";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const named = (name: string) => browser.$("main").$(`aria/${name}`);
const fen = () => named("FEN");
const notation = () => browser.$("main aside ol");
const tabs = () => browser.$$("header [role=tab]");
const tools = ["Set up position", "Copy FEN", "Copy PGN", "Paste PGN"];

const afterWhitePawn =
  "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
const empty = "8/8/8/8/8/8/8/8 w - - 0 1";
const threePieces = "4k3/8/8/8/8/8/4P3/4K3 w - - 0 1";
const drawnThreePieces = [
  "....k...",
  "........",
  "........",
  "........",
  "........",
  "........",
  "....P...",
  "....K...",
];
const drawnAfterWhitePawn = [
  "rnbqkbnr",
  "pppppppp",
  "........",
  "........",
  "....P...",
  "........",
  "PPPP.PPP",
  "RNBQKBNR",
];

async function onBoard(event: string, square: string) {
  await browser.execute(`
    const board = document.querySelector("main cg-board");
    const { x, y, width } = board.getBoundingClientRect();
    const square = "${square}";
    board.dispatchEvent(new MouseEvent("${event}", {
      bubbles: true, cancelable: true, view: window, button: 0,
      buttons: ${event === "mouseup" || event === "click" ? 0 : 1},
      clientX: x + ("abcdefgh".indexOf(square[0]) + 0.5) * (width / 8),
      clientY: y + (8 - Number(square[1]) + 0.5) * (width / 8),
    }));
  `);
}

async function clickSquare(square: string) {
  await onBoard("mousedown", square);
  await onBoard("mouseup", square);
  await onBoard("click", square);
}

async function play(from: string, to: string) {
  const moves = browser.$$("main aside li");
  const before = await moves.length;
  await clickSquare(from);
  await clickSquare(to);
  await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(before + 1);
}

async function expectBoard(rows: string[]) {
  await browser.waitUntil(
    async () => isDeepStrictEqual(await drawnBoard(), rows),
    {
      timeoutMsg: `Expected board ranks ${rows.join("/")}`,
    },
  );
}

async function expectMoves(moves: string[]) {
  await expect(browser.$$("main aside li")).toBeElementsArrayOfSize(
    moves.length,
  );
  expect(
    await browser.execute(`
    return [...document.querySelectorAll("main aside li")].map(move => move.textContent)
  `),
  ).toEqual(moves);
}

async function expectPanelTools() {
  expect(await named("Turn the board").isEnabled()).toBe(true);
  for (const name of tools) {
    expect(await named(name).isEnabled()).toBe(false);
  }
  expect(await notation().isDisplayed()).toBe(false);
}

async function copyFrom(name: string) {
  const button = await named(name).getElement();
  await expect(button).toBeEnabled();
  await browser.execute("arguments[0].focus()", button);
  await expect(button).toBeFocused();
  nativeKey("enter");
  return button;
}

async function pasteInto(name: string) {
  await browser.execute(
    "arguments[0].focus(); arguments[0].select()",
    await named(name).getElement(),
  );
  nativeKey("paste");
}

// Copy the supplied text from a separate field so the receiving field gets a real paste.
async function pasteText(name: string, text: string) {
  await browser.execute(
    `
    const source = document.createElement("textarea");
    source.value = arguments[0];
    source.addEventListener("copy", () => { source.dataset.copied = "true"; });
    document.body.append(source);
    source.focus();
    source.select();
  `,
    text,
  );
  try {
    nativeKey("copy-selection");
    await expect(browser.$("body > textarea")).toHaveAttribute(
      "data-copied",
      "true",
    );
  } finally {
    await browser.execute('document.querySelector("body > textarea").remove()');
  }
  await pasteInto(name);
  await expect(named(name)).toHaveValue(text);
}

describe("setting up a position and exchanging game text", () => {
  beforeEach(async () => {
    const before = await tabs().length;
    await browser.$("header").$("aria/New board").click();
    await expect(tabs()).toBeElementsArrayOfSize(before + 1);
    await expect(named("Set up position")).toBeEnabled();
  });

  it("sets up pieces, plays from the position, and restores the old game with undo", async () => {
    await play("e2", "e4");
    await named("Set up position").click();
    await expectPanelTools();
    await expect(fen()).toHaveValue(afterWhitePawn);
    await expectBoard(drawnAfterWhitePawn);
    await expect(
      browser.$$("aside button[aria-pressed=true]"),
    ).toBeElementsArrayOfSize(0);
    await clickSquare("e4");
    await expect(fen()).toHaveValue(afterWhitePawn);
    await clickSquare("e5");
    await expect(fen()).toHaveValue(afterWhitePawn);
    await onBoard("mousedown", "e4");
    await onBoard("mousemove", "e6");
    // The board reads the square under the pointer during its animation frame.
    await browser.pause(100);
    await onBoard("mouseup", "e6");
    await expect(fen()).toHaveValue(
      "rnbqkbnr/pppppppp/4P3/8/8/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
    );

    await named("Empty board").click();
    await expect(fen()).toHaveValue(empty);
    await expectBoard(Array(8).fill("........"));
    await named("White king").click();
    await clickSquare("e1");
    await expect(fen()).toHaveValue("8/8/8/8/8/8/8/4K3 w - - 0 1");
    await named("Black king").click();
    await clickSquare("e8");
    await expect(fen()).toHaveValue("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    await named("White pawn").click();
    await clickSquare("e2");
    await named("White").click();
    await expect(fen()).toHaveValue(threePieces);
    await named("Eraser").click();
    await clickSquare("e2");
    await expect(fen()).toHaveValue("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    await named("White pawn").click();
    await clickSquare("e2");
    await expect(fen()).toHaveValue(threePieces);
    await expectBoard(drawnThreePieces);
    mkdirSync(screenshots, { recursive: true });
    const image = await browser.saveScreenshot(`${screenshots}/set-up.png`);
    expect([image.readUInt32BE(16), image.readUInt32BE(20)]).toEqual([
      1280, 800,
    ]);

    const setupTab = await tabs().length;
    await browser.$("header").$("aria/New board").click();
    await expect(named("Set up position")).toBeEnabled();
    await browser
      .$(`header [role=tablist] > :nth-child(${setupTab}) [role=tab]`)
      .click();
    await expect(fen()).toHaveValue(threePieces);
    await expectBoard(drawnThreePieces);

    await named("Use position").click();
    await expect(named("Set up position")).toBeEnabled();
    await expectMoves([]);
    await expectBoard(drawnThreePieces);
    await browser.keys(["Control", "z"]);
    await expectMoves(["1.e4"]);
    await expectBoard(drawnAfterWhitePawn);
    await browser.keys(["Control", "y"]);
    await expectMoves([]);
    await expectBoard(drawnThreePieces);
    await clickSquare("e2");
    await expect(
      browser.$$("main cg-board square.move-dest"),
    ).toBeElementsArrayOfSize(2);
    const targets = await browser.execute(`
      const board = document.querySelector("main cg-board").getBoundingClientRect();
      return [...document.querySelectorAll("main cg-board square.move-dest")]
        .map(square => square.getBoundingClientRect()).filter(square => square.width > 0)
        .map(square => "abcdefgh"[Math.round((square.x-board.x) / (board.width/8))]
          + (8 - Math.round((square.y-board.y) / (board.height/8)))).sort();
    `);
    expect(targets).toEqual(["e3", "e4"]);
    await clickSquare("e3");
    await expectMoves(["1.e3"]);
  });

  it("keeps an illegal position in setup until the player cancels", async () => {
    await play("e2", "e4");
    await named("Set up position").click();
    await named("Empty board").click();
    await named("Use position").click();
    await expect(browser.$("main aside [role=alert]")).toHaveText(
      "This is not a legal position.",
    );
    await expect(fen()).toHaveValue(empty);
    await named("Cancel").click();
    await expect(named("Set up position")).toBeEnabled();
    await expectMoves(["1.e4"]);
    await expectBoard(drawnAfterWhitePawn);
  });

  it("updates the board and its fields from FEN and restores the start position", async () => {
    await named("Set up position").click();
    await pasteText("FEN", "4k3/4p3/8/8/8/8/8/4K3 b - - 0 12");
    const pasted = [
      "....k...",
      "....p...",
      "........",
      "........",
      "........",
      "........",
      "........",
      "....K...",
    ];
    await expectBoard(pasted);
    expect(await named("Black").isSelected()).toBe(true);
    await fen().setValue("broken");
    await expect(fen()).toHaveAttribute("aria-invalid", "true");
    await expectBoard(pasted);
    await expect(named("Use position")).toBeEnabled();
    await named("Use position").click();
    await expect(named("Set up position")).toBeEnabled();
    await expectBoard(pasted);
    await named("Set up position").click();
    await expect(fen()).toHaveValue("4k3/4p3/8/8/8/8/8/4K3 b - - 0 12");
    await named("Start position").click();
    await expectBoard(startPositionWithWhiteAtBottom);
    await expect(fen()).toHaveValue(
      "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    );
    for (const right of [
      "White short",
      "White long",
      "Black short",
      "Black long",
    ]) {
      expect(await named(right).isSelected()).toBe(true);
    }
    await named("Black").click();
    await named("White short").click();
    await named("En passant square").setValue("e3");
    await expect(fen()).toHaveValue(
      "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b Qkq e3 0 1",
    );
    await named("Cancel").click();
    await expect(named("Set up position")).toBeEnabled();
  });

  it("pastes the exact FEN and PGN copied by the board buttons", async () => {
    await play("e2", "e4");
    await play("e7", "e5");
    const copyFen = await copyFrom("Copy FEN");
    await expect(copyFen).toHaveText("Copied");
    await expect(copyFen).toHaveText("Copy FEN");
    await named("Paste PGN").click();
    await expectPanelTools();
    await pasteInto("PGN text");
    await expect(named("PGN text")).toHaveValue(
      "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
    );
    await named("Cancel").click();

    const copyPgn = await copyFrom("Copy PGN");
    await expect(copyPgn).toHaveText("Copied");
    await named("Paste PGN").click();
    await pasteInto("PGN text");
    await expect(named("PGN text")).toHaveValue("1. e4 e5 *\n", {
      trim: false,
    });
    await named("Open as new game").click();
    await expectMoves(["1.e4", "e5"]);
  });

  it("opens named and partial games in new tabs and explains unreadable text", async () => {
    const before = await tabs().length;
    await named("Paste PGN").click();
    await pasteText(
      "PGN text",
      '[White "Alpha"]\n[Black "Beta"]\n\n1. d4 d5 2. c4 *',
    );
    await named("Open as new game").click();
    await expect(tabs()).toBeElementsArrayOfSize(before + 1);
    await expect(browser.$("header [role=tab][aria-selected=true]")).toHaveText(
      "Alpha - Beta",
    );
    await expectMoves(["1.d4", "d5", "2.c4"]);

    await named("Paste PGN").click();
    await pasteText("PGN text", "hello");
    await named("Open as new game").click();
    await expect(browser.$("main aside [role=alert]")).toHaveText(
      "This text holds no game.",
    );
    await expect(named("PGN text")).toBeDisplayed();
    await expect(tabs()).toBeElementsArrayOfSize(before + 1);
    await pasteText("PGN text", "1. e4 e5 2. Ke3 Nc6 *");
    await named("Open as new game").click();
    await expect(tabs()).toBeElementsArrayOfSize(before + 2);
    await expectMoves(["1.e4", "e5"]);
    await expect(browser.$("main aside [role=alert]")).toHaveText(
      `illegal move "Ke3" at White's move 2, 2 moves kept`,
    );
  });

  it("shows the writer's reason and keeps the clipboard when a game cannot be copied", async () => {
    await named("Paste PGN").click();
    await pasteText("PGN text", '[FEN "broken"]\n\n*');
    await named("Open as new game").click();
    await expect(named("Set up position")).toBeEnabled();
    await play("e2", "e4");
    await copyFrom("Copy PGN");
    await expect(browser.$("main > div [role=alert]")).toHaveText(
      "This game cannot be written as PGN. invalid FEN would discard the move text",
    );
    await named("Paste PGN").click();
    await pasteInto("PGN text");
    await expect(named("PGN text")).toHaveValue('[FEN "broken"]\n\n*');
    await named("Cancel").click();
  });
});
