import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);

const board = () => browser.$("main cg-board");
const tab = (positionFromLeft: number) =>
  browser.$(
    `header [role="tablist"] > :nth-child(${positionFromLeft}) [role="tab"]`,
  );
/** The whole text of each move in the notation, with its number where it has one. */
async function writtenMoves() {
  // The test code has no types of the window, so the script is a string and its result has no type.
  const moves: unknown = await browser.execute(`
    return [...document.querySelectorAll("main aside li")].map((move) => move.textContent);
  `);
  return moves as string[];
}

/** Where the middle of a square lies from the middle of the board, with White at the bottom. */
async function fromMiddleOfBoard(square: string) {
  const squareSide = (await board().getSize("width")) / 8;
  const fromFileA = "abcdefgh".indexOf(square.charAt(0));
  const fromRank1 = Number(square.charAt(1)) - 1;
  return {
    x: (fromFileA - 3.5) * squareSide,
    y: (3.5 - fromRank1) * squareSide,
  };
}

async function clickSquare(square: string) {
  await board().click(await fromMiddleOfBoard(square));
}

async function playMove(from: string, to: string) {
  const movesBefore = (await writtenMoves()).length;
  await clickSquare(from);
  await clickSquare(to);
  await browser.waitUntil(
    async () => (await writtenMoves()).length === movesBefore + 1,
  );
}

// The board works out the square under the mouse once per frame, so the mouse rests before
// the button goes up.
const oneFrame = 100;

/** Draws the way a player does: a drag of the right button, or a click of it on one square. */
async function drawWithRightButton(from: string, to: string = from) {
  const drawing = browser
    .action("pointer")
    .move({ origin: board(), ...(await fromMiddleOfBoard(from)) })
    .down({ button: 2 })
    .move({
      origin: board(),
      ...(await fromMiddleOfBoard(to)),
      duration: oneFrame,
    });
  await drawing.pause(oneFrame).up({ button: 2 }).perform();
}

/**
 * Draws with Shift or Alt held. The driver of the test makes mouse events that carry no key,
 * so these mouse events go to the board from here.
 */
async function drawWithKeys(
  keys: { shift: boolean; alt: boolean },
  from: string,
  to: string,
) {
  const send = (event: string, square: string) =>
    browser.execute(`
      const board = document.querySelector("main cg-board");
      const { x, y, width } = board.getBoundingClientRect();
      const square = "${square}";
      board.dispatchEvent(new MouseEvent("${event}", {
        bubbles: true, cancelable: true, view: window, button: 2, buttons: 2,
        shiftKey: ${keys.shift}, altKey: ${keys.alt},
        clientX: x + ("abcdefgh".indexOf(square[0]) + 0.5) * (width / 8),
        clientY: y + (8 - Number(square[1]) + 0.5) * (width / 8),
      }));
    `);
  await send("mousedown", from);
  await send("mousemove", to);
  await browser.pause(oneFrame);
  await send("mouseup", to);
}

/** The shapes that the board draws, each as its color and its squares, in the order of the text. */
const shapes = () =>
  browser.execute(`
    const colors = { "#15781B": "green", "#882020": "red", "#003088": "blue", "#e68f00": "yellow" };
    const square = (x, y) =>
      "abcdefgh"[Math.round(Number(x) + 3.5)] + (Math.round(3.5 - Number(y)) + 1);
    const read = (shape, name) => shape.getAttribute(name);
    return [...document.querySelectorAll("main svg.cg-shapes > g > g")]
      .map((drawn) => {
        const arrow = drawn.querySelector("line");
        if (arrow !== null) {
          const from = square(read(arrow, "x1"), read(arrow, "y1"));
          const to = square(read(arrow, "x2"), read(arrow, "y2"));
          return colors[read(arrow, "stroke")] + " " + from + "-" + to;
        }
        const circle = drawn.querySelector("circle");
        return colors[read(circle, "stroke")] + " " + square(read(circle, "cx"), read(circle, "cy"));
      })
      .sort();
  `);

async function expectShapes(expected: string[]) {
  const giveUpAt = Date.now() + 3000;
  while (
    !isDeepStrictEqual(await shapes(), expected) &&
    Date.now() < giveUpAt
  ) {
    await browser.pause(50);
  }
  expect(await shapes()).toEqual(expected);
}

describe("the shapes on the board", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    mkdirSync(screenshots, { recursive: true });
    await browser.$("header").$("aria/New board").click();
    await expect(board()).toExist();
    await playMove("e2", "e4");
  });

  it("draws an arrow with a drag of the right button and marks a square with a click", async () => {
    await expectShapes([]);

    await drawWithRightButton("g1", "f3");
    await expectShapes(["green g1-f3"]);

    await drawWithRightButton("d4");
    await expectShapes(["green d4", "green g1-f3"]);
  });

  it("draws in red with Shift, in blue with Alt, and in yellow with both", async () => {
    await drawWithKeys({ shift: true, alt: false }, "d1", "h5");
    await expectShapes(["green d4", "green g1-f3", "red d1-h5"]);

    const picture = await browser.saveScreenshot(`${screenshots}/shapes.png`);
    // A PNG file holds its width and its height at these two places.
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });

    await drawWithKeys({ shift: false, alt: true }, "b1", "c3");
    await drawWithKeys({ shift: true, alt: true }, "a2", "a4");
    await expectShapes([
      "blue b1-c3",
      "green d4",
      "green g1-f3",
      "red d1-h5",
      "yellow a2-a4",
    ]);
  });

  it("writes no text for the shapes into the notation", async () => {
    expect(await writtenMoves()).toEqual(["1.e4"]);
  });

  it("draws the shapes of the picture that Rust answered when the tab is shown again", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(2)).toHaveAttribute("aria-selected", "true");
    await expectShapes([]);

    await tab(1).click();
    await expectShapes([
      "blue b1-c3",
      "green d4",
      "green g1-f3",
      "red d1-h5",
      "yellow a2-a4",
    ]);
  });

  it("takes a shape away when it is drawn again, and all with a left click on an empty square", async () => {
    await drawWithRightButton("g1", "f3");
    await expectShapes(["blue b1-c3", "green d4", "red d1-h5", "yellow a2-a4"]);

    await clickSquare("e5");
    await expectShapes([]);
  });

  it("shows no shape at the place behind a move", async () => {
    await drawWithRightButton("d4");
    await expectShapes(["green d4"]);

    await playMove("e7", "e5");
    await expectShapes([]);
    expect(await writtenMoves()).toEqual(["1.e4", "e5"]);
  });

  it("shows the shapes of a place again when the game goes back to it", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
    await playMove("e2", "e4");
    await drawWithRightButton("g1", "f3");
    await expectShapes(["green g1-f3"]);
    await playMove("e7", "e5");
    await expectShapes([]);

    await browser.keys("ArrowLeft");

    await expectShapes(["green g1-f3"]);
  });

  it("brings the shapes back with Ctrl+Z after a click took them away", async () => {
    await clickSquare("e5");
    await expectShapes([]);

    await browser.keys(["Control", "z"]);

    await expectShapes(["green g1-f3"]);
  });
});
