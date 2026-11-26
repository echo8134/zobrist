import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);

const board = () => browser.$("main cg-board");
const inNotationArea = (name: string) =>
  browser.$("main aside").$(`aria/${name}`);
const commentField = () => inNotationArea("Comment");

async function clickSquare(square: string) {
  const squareSide = (await board().getSize("width")) / 8;
  await board().click({
    x: ("abcdefgh".indexOf(square.charAt(0)) - 3.5) * squareSide,
    y: (4.5 - Number(square.charAt(1))) * squareSide,
  });
}

async function playMoves(moves: string) {
  const writtenMoves = () => browser.$$("main aside li").length;
  let expectedMoves = await writtenMoves();
  for (const move of moves.split(" ")) {
    await clickSquare(move.slice(0, 2));
    await clickSquare(move.slice(2));
    expectedMoves += 1;
    await browser.waitUntil(
      async () => (await writtenMoves()) === expectedMoves,
      { interval: 10, timeoutMsg: `the notation did not get the move ${move}` },
    );
  }
}

// A click of a mouse takes the focus from the field before it does anything else. The click
// of the driver leaves the focus where it is, so the test takes it away.
async function clickOnTheBoard() {
  await browser.execute("document.activeElement.blur()");
  await clickSquare("a4");
}

const seen = () =>
  browser.execute(`
    const texts = document.createTreeWalker(document.querySelector("main aside ol"), NodeFilter.SHOW_TEXT);
    const parts = [];
    while (texts.nextNode()) {
      const text = texts.currentNode.textContent;
      const isCurrent = texts.currentNode.parentElement.hasAttribute("aria-current");
      parts.push(isCurrent ? "[" + text + "]" : text);
    }
    const symbolButtons = [...document.querySelectorAll("main aside [aria-pressed]")];
    return {
      notation: parts.join(" ").replaceAll("( ", "(").replaceAll(" )", ")"),
      field: document.querySelector("main aside textarea").value,
      setSymbols: symbolButtons
        .filter((button) => button.getAttribute("aria-pressed") === "true")
        .map((button) => button.textContent),
      dimSymbols: symbolButtons.filter((button) => button.disabled).length,
    };
  `);

async function expectSoon(read: () => Promise<unknown>, expected: unknown) {
  const giveUpAt = Date.now() + 3000;
  while (!isDeepStrictEqual(await read(), expected) && Date.now() < giveUpAt) {
    await browser.pause(50);
  }
  expect(await read()).toEqual(expected);
}

describe("the comments and the symbols of a game", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    mkdirSync(screenshots, { recursive: true });
    await browser.$("header").$("aria/New board").click();
    await expect(board()).toExist();
    await playMoves("e2e4 e7e5");
  });

  it("names the field and the fourteen symbol buttons", async () => {
    expect(await commentField().getTagName()).toBe("textarea");
    expect(
      await browser
        .$$("main aside [aria-pressed]")
        .map(async (button) => [
          await button.getText(),
          await button.getComputedLabel(),
        ]),
    ).toEqual([
      ["!", "Good move"],
      ["?", "Mistake"],
      ["!!", "Brilliant move"],
      ["??", "Blunder"],
      ["!?", "Interesting move"],
      ["?!", "Dubious move"],
      ["=", "Equal position"],
      ["∞", "Unclear position"],
      ["⩲", "White is slightly better"],
      ["⩱", "Black is slightly better"],
      ["±", "White is better"],
      ["∓", "Black is better"],
      ["+-", "White is winning"],
      ["-+", "Black is winning"],
    ]);
  });

  it("writes the comment of the field behind the current move when the field loses the focus", async () => {
    await commentField().click();
    await commentField().setValue("Open game");
    await clickOnTheBoard();

    await expectSoon(seen, {
      notation: "1. e4 [e5] Open game",
      field: "Open game",
      setSymbols: [],
      dimSymbols: 0,
    });
    expect(
      await browser.execute(
        `return getComputedStyle([...document.querySelectorAll("main aside li > span")].find((written) => written.textContent === "Open game")).color`,
      ),
    ).toBe("rgb(63, 122, 86)");
  });

  it("shows the comment of the current place in the field and takes a comment with Ctrl+Enter", async () => {
    await inNotationArea("e4").click();
    await expectSoon(seen, {
      notation: "1. [e4] e5 Open game",
      field: "",
      setSymbols: [],
      dimSymbols: 0,
    });

    await commentField().click();
    await commentField().setValue("Best by test");
    await browser.keys(["Control", "Enter"]);

    await expectSoon(seen, {
      notation: "1. [e4] Best by test 1... e5 Open game",
      field: "Best by test",
      setSymbols: [],
      dimSymbols: 0,
    });
  });

  it("shows the comment of the picture again when Rust refuses the text of the field", async () => {
    await commentField().setValue("Best } by test");
    await browser.keys(["Control", "Enter"]);

    await expectSoon(seen, {
      notation: "1. [e4] Best by test 1... e5 Open game",
      field: "Best by test",
      setSymbols: [],
      dimSymbols: 0,
    });
  });

  it("leaves the left arrow to the field while the field has the focus", async () => {
    await browser.execute(`
      window.arrowWasLeftToTheField = undefined;
      document.addEventListener(
        "keydown",
        (event) => { window.arrowWasLeftToTheField = !event.defaultPrevented; },
        { once: true },
      );
    `);

    await browser.keys("ArrowLeft");

    expect(await browser.execute("return window.arrowWasLeftToTheField")).toBe(
      true,
    );
    expect(await seen()).toEqual({
      notation: "1. [e4] Best by test 1... e5 Open game",
      field: "Best by test",
      setSymbols: [],
      dimSymbols: 0,
    });
  });

  it("sets a symbol of the move with a click and takes it away with a second click", async () => {
    await inNotationArea("Good move").click();
    await expectSoon(seen, {
      notation: "1. [e4!] Best by test 1... e5 Open game",
      field: "Best by test",
      setSymbols: ["!"],
      dimSymbols: 0,
    });

    await inNotationArea("Blunder").click();
    await expectSoon(seen, {
      notation: "1. [e4??] Best by test 1... e5 Open game",
      field: "Best by test",
      setSymbols: ["??"],
      dimSymbols: 0,
    });

    await inNotationArea("Blunder").click();
    await expectSoon(seen, {
      notation: "1. [e4] Best by test 1... e5 Open game",
      field: "Best by test",
      setSymbols: [],
      dimSymbols: 0,
    });
  });

  it("sets a symbol of the move and a symbol of the position at one move", async () => {
    await inNotationArea("e5").click();
    await inNotationArea("Dubious move").click();
    await inNotationArea("White is slightly better").click();

    await expectSoon(seen, {
      notation: "1. e4 Best by test 1... [e5?! ⩲] Open game",
      field: "Open game",
      setSymbols: ["?!", "⩲"],
      dimSymbols: 0,
    });
    await expect(
      browser.$$("main cg-board piece.anim"),
    ).toBeElementsArrayOfSize(0);
    const picture = await browser.saveScreenshot(`${screenshots}/comments.png`);
    // A PNG file holds its width and its height at these two places.
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });

    await inNotationArea("e4").click();

    await expectSoon(seen, {
      notation: "1. [e4] Best by test 1... e5?! ⩲ Open game",
      field: "Best by test",
      setSymbols: [],
      dimSymbols: 0,
    });
  });

  it("dims the symbol buttons at the start and writes the comment of the start before the first move", async () => {
    await browser.keys("Home");
    await expectSoon(seen, {
      notation: "1. e4 Best by test 1... e5?! ⩲ Open game",
      field: "",
      setSymbols: [],
      dimSymbols: 14,
    });

    await commentField().click();
    await commentField().setValue("A short game");
    await browser.keys(["Control", "Enter"]);

    await expectSoon(seen, {
      notation: "A short game 1. e4 Best by test 1... e5?! ⩲ Open game",
      field: "A short game",
      setSymbols: [],
      dimSymbols: 14,
    });
  });

  it("takes a comment away when the field is empty, and Ctrl+Z brings it back", async () => {
    await inNotationArea("e5?! ⩲").click();
    await expectSoon(seen, {
      notation: "A short game 1. e4 Best by test 1... [e5?! ⩲] Open game",
      field: "Open game",
      setSymbols: ["?!", "⩲"],
      dimSymbols: 0,
    });

    await commentField().click();
    await commentField().clearValue();
    await browser.keys(["Control", "Enter"]);

    await expectSoon(seen, {
      notation: "A short game 1. e4 Best by test 1... [e5?! ⩲]",
      field: "",
      setSymbols: ["?!", "⩲"],
      dimSymbols: 0,
    });

    await clickOnTheBoard();
    await browser.keys(["Control", "z"]);

    await expectSoon(seen, {
      notation: "A short game 1. e4 Best by test 1... [e5?! ⩲] Open game",
      field: "Open game",
      setSymbols: ["?!", "⩲"],
      dimSymbols: 0,
    });
  });

  it("lays out the panel below the notation", async () => {
    expect(await commentField().getSize()).toEqual({
      width: 320 - 1 - 2 * 16,
      height: 64,
    });
    expect(await commentField().getLocation()).toEqual({
      x: 960 + 1 + 16,
      y: 800 - 12 - 28 - 8 - 28 - 8 - 64,
    });
    const goodMove = inNotationArea("Good move");
    expect(await goodMove.getSize()).toEqual({ width: 28, height: 28 });
    expect(await goodMove.getLocation()).toEqual({
      x: 960 + 1 + 16,
      y: 800 - 12 - 28 - 8 - 28,
    });
    expect(await inNotationArea("Mistake").getLocation("x")).toBe(
      960 + 1 + 16 + 28 + 2,
    );
    expect(await inNotationArea("Equal position").getLocation()).toEqual({
      x: 960 + 1 + 16,
      y: 800 - 12 - 28,
    });
  });
});
