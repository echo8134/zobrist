import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);

const board = () => browser.$("main cg-board");
const tool = (name: string) => browser.$("main aside").$(`aria/${name}`);

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

// A game without changes has nothing to save.
const saveTools = ["Save", "Save as new game"];
const allTools = [
  "Undo",
  "Redo",
  "Promote variation",
  "Delete variation",
  "Delete the rest",
];

const seen = () =>
  browser.execute(`
    const texts = document.createTreeWalker(document.querySelector("main aside ol"), NodeFilter.SHOW_TEXT);
    const parts = [];
    while (texts.nextNode()) {
      const text = texts.currentNode.textContent;
      const isCurrent = texts.currentNode.parentElement.hasAttribute("aria-current");
      parts.push(isCurrent ? "[" + text + "]" : text);
    }
    return {
      notation: parts.join(" ").replaceAll("( ", "(").replaceAll(" )", ")"),
      dimTools: [...document.querySelectorAll("main aside > div:first-child button:disabled")].map(
        (button) => button.getAttribute("aria-label"),
      ),
    };
  `);

async function expectSoon(read: () => Promise<unknown>, expected: unknown) {
  const giveUpAt = Date.now() + 3000;
  while (!isDeepStrictEqual(await read(), expected) && Date.now() < giveUpAt) {
    await browser.pause(50);
  }
  expect(await read()).toEqual(expected);
}

describe("the tools above the notation", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    mkdirSync(screenshots, { recursive: true });
    await browser.$("header").$("aria/New board").click();
    await expect(board()).toExist();
  });

  it("shows seven dim buttons above the notation of a new game", async () => {
    expect(
      await browser
        .$$("main aside > div:first-child button")
        .map((button) => button.getComputedLabel()),
    ).toEqual([...saveTools, ...allTools]);
    expect(await seen()).toEqual({
      notation: "",
      dimTools: [...saveTools, ...allTools],
    });
    expect(await tool("Undo").isEnabled()).toBe(false);
  });

  it("takes back the last change with Undo and brings it back with Redo", async () => {
    await playMoves("e2e4 e7e5");
    await expectSoon(seen, {
      notation: "1. e4 [e5]",
      dimTools: [
        "Redo",
        "Promote variation",
        "Delete variation",
        "Delete the rest",
      ],
    });

    await tool("Undo").click();

    await expectSoon(seen, {
      notation: "1. [e4]",
      dimTools: ["Promote variation", "Delete variation", "Delete the rest"],
    });
    expect(await browser.$$("main cg-board piece.black.pawn").length).toBe(8);

    await tool("Redo").click();

    await expectSoon(seen, {
      notation: "1. e4 [e5]",
      dimTools: [
        "Redo",
        "Promote variation",
        "Delete variation",
        "Delete the rest",
      ],
    });
  });

  it("takes back with Ctrl+Z and brings back with Ctrl+Y and with Ctrl+Shift+Z", async () => {
    await browser.keys(["Control", "z"]);
    await browser.keys(["Control", "z"]);

    await expectSoon(seen, {
      notation: "",
      dimTools: [
        ...saveTools,
        "Undo",
        "Promote variation",
        "Delete variation",
        "Delete the rest",
      ],
    });
    expect(await browser.$$("main cg-board piece").length).toBe(32);

    await browser.keys(["Control", "y"]);

    await expectSoon(seen, {
      notation: "1. [e4]",
      dimTools: ["Promote variation", "Delete variation", "Delete the rest"],
    });

    await browser.keys(["Control", "Shift", "z"]);

    await expectSoon(seen, {
      notation: "1. e4 [e5]",
      dimTools: [
        "Redo",
        "Promote variation",
        "Delete variation",
        "Delete the rest",
      ],
    });
  });

  it("makes the variation of the current move the line", async () => {
    await playMoves("g1f3");
    await tool("e4").click();
    await expectSoon(seen, {
      notation: "1. [e4] e5 2. Nf3",
      dimTools: ["Redo", "Promote variation", "Delete variation"],
    });
    await playMoves("c7c5");
    await expectSoon(seen, {
      notation: "1. e4 e5 (1... [c5]) 2. Nf3",
      dimTools: ["Redo", "Delete the rest"],
    });
    await expect(
      browser.$$("main cg-board piece.anim"),
    ).toBeElementsArrayOfSize(0);
    const picture = await browser.saveScreenshot(
      `${screenshots}/notation-tools.png`,
    );
    // A PNG file holds its width and its height at these two places.
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });

    await tool("Promote variation").click();

    await expectSoon(seen, {
      notation: "1. e4 [c5] (1... e5 2. Nf3)",
      dimTools: [
        "Redo",
        "Promote variation",
        "Delete variation",
        "Delete the rest",
      ],
    });
  });

  it("deletes the variation of the current move", async () => {
    await tool("Undo").click();
    await expectSoon(seen, {
      notation: "1. e4 e5 (1... [c5]) 2. Nf3",
      dimTools: ["Delete the rest"],
    });

    await tool("Delete variation").click();

    await expectSoon(seen, {
      notation: "1. [e4] e5 2. Nf3",
      dimTools: ["Redo", "Promote variation", "Delete variation"],
    });
  });

  it("deletes every move after the current one", async () => {
    await tool("Delete the rest").click();

    await expectSoon(seen, {
      notation: "1. [e4]",
      dimTools: [
        "Redo",
        "Promote variation",
        "Delete variation",
        "Delete the rest",
      ],
    });

    await tool("Undo").click();

    await expectSoon(seen, {
      notation: "1. [e4] e5 2. Nf3",
      dimTools: ["Promote variation", "Delete variation"],
    });
  });

  it("takes the keys only while its tab is shown", async () => {
    await browser.$("nav").$("aria/Files").click();
    await browser.keys(["Control", "z"]);
    await browser
      .$('header [role="tablist"] > :nth-child(1) [role="tab"]')
      .click();

    await expectSoon(seen, {
      notation: "1. [e4] e5 2. Nf3",
      dimTools: ["Promote variation", "Delete variation"],
    });
  });

  it("lays out the row of buttons", async () => {
    const row = browser.$("main aside > div");
    expect(await row.getSize()).toEqual({ width: 320 - 1, height: 40 });
    expect(await row.getLocation()).toEqual({ x: 1280 - 320 + 1, y: 40 });
    const left = async (name: string) => tool(name).getLocation("x");
    expect({
      undo: await left("Undo"),
      redo: await left("Redo"),
      promote: await left("Promote variation"),
      deleteVariation: await left("Delete variation"),
      deleteTheRest: await left("Delete the rest"),
    }).toEqual({
      // The two save buttons and the gap behind them stand before Undo.
      undo: 960 + 1 + 8 + 2 * 32 + 2 + 12,
      redo: 960 + 1 + 8 + 3 * 32 + 2 * 2 + 12,
      promote: 960 + 1 + 8 + 4 * 32 + 2 * 2 + 2 * 12,
      deleteVariation: 960 + 1 + 8 + 5 * 32 + 3 * 2 + 2 * 12,
      deleteTheRest: 960 + 1 + 8 + 6 * 32 + 4 * 2 + 2 * 12,
    });
  });
});
