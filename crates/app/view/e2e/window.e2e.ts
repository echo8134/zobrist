import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";
import { drawnBoard, startPositionWithWhiteAtBottom } from "./board";
import { nativeKey } from "./native-key";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);

// A release changes only the version line of the root manifest, so the expected value comes
// from there and not from a literal.
const manifest = readFileSync(
  new URL("../../../../Cargo.toml", import.meta.url),
  "utf8",
);
const version = /\[workspace\.package\][^[]*?^version = "([^"]+)"$/m.exec(
  manifest,
)?.[1];
if (version === undefined) {
  throw new Error("[workspace.package] of the root manifest sets no version");
}

// An entry and the toggle are found by the name that assistive technology reads, because a
// collapsed entry shows no text.
const toggle = (name: string) => browser.$("header").$(`aria/${name}`);
const entry = (name: string) => browser.$("nav").$(`aria/${name}`);
const entryNamesAreDisplayed = () =>
  browser.$$("nav button span").map((name) => name.isDisplayed());
const currentEntries = () =>
  browser
    .$$('nav button[aria-current="page"]')
    .map((button) => button.getComputedLabel());
const heading = () => browser.$("main").$("h1");

const tabs = () => browser.$$('header [role="tablist"] [role="tab"]');
const tab = (positionFromLeft: number) =>
  browser.$(
    `header [role="tablist"] > :nth-child(${positionFromLeft}) [role="tab"]`,
  );
const closeButton = (positionFromLeft: number) =>
  browser
    .$(`header [role="tablist"] > :nth-child(${positionFromLeft})`)
    .$("aria/Close New game");
const plusButton = () => browser.$("header").$("aria/New board");

const turnButton = () => browser.$("main").$("aria/Turn the board");

async function openTabAndWait() {
  const tabsBefore = await tabs().length;
  await plusButton().click();
  await expect(tabs()).toBeElementsArrayOfSize(tabsBefore + 1);
}

const shown = async () => ({
  headings: await browser.$$("main h1").map((heading) => heading.getText()),
  boards: await browser.$$("main cg-board").length,
  currentEntries: await currentEntries(),
  tabsAreSelected: await tabs().map(
    async (tab) => (await tab.getAttribute("aria-selected")) === "true",
  ),
});

const startPositionWithBlackAtBottom = [
  "RNBKQBNR",
  "PPPPPPPP",
  "........",
  "........",
  "........",
  "........",
  "pppppppp",
  "rnbkqbnr",
];

const drawnCoordinates = () =>
  browser.execute(`
    const inDrawnOrder = (kind, axis) =>
      [...document.querySelectorAll("main coords." + kind + " coord")]
        .sort((one, other) => one.getBoundingClientRect()[axis] - other.getBoundingClientRect()[axis])
        .map((coordinate) => coordinate.textContent)
        .join("");
    return { fromLeft: inDrawnOrder("files", "x"), fromTop: inDrawnOrder("ranks", "y") };
  `);

type BottomSide = "white" | "black";

async function offsetFromMiddleOfBoard(square: string, bottomSide: BottomSide) {
  const squareSide = (await browser.$("main cg-board").getSize("width")) / 8;
  const fromFileA = "abcdefgh".indexOf(square.charAt(0));
  const fromRank1 = Number(square.charAt(1)) - 1;
  const fromLeft = bottomSide === "white" ? fromFileA : 7 - fromFileA;
  const fromTop = bottomSide === "white" ? 7 - fromRank1 : fromRank1;
  return { x: (fromLeft - 3.5) * squareSide, y: (fromTop - 3.5) * squareSide };
}

async function clickSquare(square: string, bottomSide: BottomSide = "white") {
  await browser
    .$("main cg-board")
    .click(await offsetFromMiddleOfBoard(square, bottomSide));
}

async function dragPiece(
  from: string,
  to: string,
  bottomSide: BottomSide = "white",
) {
  const board = browser.$("main cg-board");
  await browser
    .action("pointer")
    .move({
      origin: board,
      ...(await offsetFromMiddleOfBoard(from, bottomSide)),
    })
    .down()
    .move({
      origin: board,
      ...(await offsetFromMiddleOfBoard(to, bottomSide)),
      duration: 100,
    })
    .up()
    .perform();
}

async function playMoves(moves: string, bottomSide: BottomSide = "white") {
  const writtenMoves = () => browser.$$("main aside li").length;
  let expectedMoves = await writtenMoves();
  for (const move of moves.split(" ")) {
    await clickSquare(move.slice(0, 2), bottomSide);
    await clickSquare(move.slice(2), bottomSide);
    expectedMoves += 1;
    // The other side can move only after the answer of Rust, and the written move shows it.
    await browser.waitUntil(
      async () => (await writtenMoves()) === expectedMoves,
      { interval: 10, timeoutMsg: `the notation did not get the move ${move}` },
    );
  }
}

const readNotation = `
  const notationText = (currentInBrackets) => {
    const texts = document.createTreeWalker(document.querySelector("main aside ol"), NodeFilter.SHOW_TEXT);
    const parts = [];
    while (texts.nextNode()) {
      const text = texts.currentNode.textContent;
      const isCurrent = texts.currentNode.parentElement.hasAttribute("aria-current");
      parts.push(currentInBrackets && isCurrent ? "[" + text + "]" : text);
    }
    return parts.join(" ").replaceAll("( ", "(").replaceAll(" )", ")");
  };
`;
const markedNotation = () =>
  browser.execute(`${readNotation} return notationText(true);`);
const moveInNotation = (name: string) =>
  browser.$("main aside").$(`aria/${name}`);

// A mark of the board counts only while it is drawn, because the board component keeps the
// element of a mark that is gone.
const game = (squares: string, bottomSide: BottomSide = "white") =>
  browser.execute(`
    ${readNotation}
    const board = document.querySelector("main cg-board").getBoundingClientRect();
    const squareOf = (element) => {
      const { x, y } = element.getBoundingClientRect();
      const fromLeft = Math.round((x - board.x) / (board.width / 8));
      const fromTop = Math.round((y - board.y) / (board.height / 8));
      return ${bottomSide === "white"}
        ? "abcdefgh"[fromLeft] + (8 - fromTop)
        : "hgfedcba"[fromLeft] + (fromTop + 1);
    };
    const letters = { pawn: "p", knight: "n", bishop: "b", rook: "r", queen: "q", king: "k" };
    const pieces = Object.fromEntries("${squares}".split(" ").map((square) => [square, "."]));
    for (const piece of document.querySelectorAll("main cg-board piece")) {
      const letter = letters[piece.classList[1]];
      const square = squareOf(piece);
      if (square in pieces) {
        pieces[square] = piece.classList[0] === "white" ? letter.toUpperCase() : letter;
      }
    }
    const marked = (mark) =>
      [...document.querySelectorAll("main cg-board square." + mark)]
        .filter((square) => square.getBoundingClientRect().width > 0)
        .map(squareOf)
        .sort()
        .join(" ");
    return {
      pieces,
      lastMove: marked("last-move"),
      targets: marked("move-dest"),
      selected: marked("selected"),
      check: marked("check"),
      notation: notationText(false),
      currentMoves: [...document.querySelectorAll("main aside [aria-current]")].map(
        (move) => move.textContent,
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

/** Saves a picture of the window and returns its size in pixels. */
async function screenshot(name: string) {
  const picture = await browser.saveScreenshot(`${screenshots}/${name}.png`);
  // A PNG file holds its width and its height at these two places.
  return { width: picture.readUInt32BE(16), height: picture.readUInt32BE(20) };
}

describe("the window", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    mkdirSync(screenshots, { recursive: true });
  });

  it("opens with the top bar, the icon bar, and the Home page", async () => {
    await expect(browser.$("header")).toHaveText("zobrist");
    await expect(heading()).toHaveText("Home");
    expect(
      await browser.$$("nav button").map((button) => button.getText()),
    ).toEqual([
      "Home",
      "Files",
      "Databases",
      "Events",
      "Engines",
      "User",
      "Settings",
    ]);
    expect(await entryNamesAreDisplayed()).toEqual([
      true,
      true,
      true,
      true,
      true,
      true,
      true,
    ]);
    expect(await currentEntries()).toEqual(["Home"]);
    expect(await tabs().length).toBe(0);
    expect(await plusButton().getLocation("x")).toBe(208 + 8);

    expect(await screenshot("home")).toEqual({ width: 1280, height: 800 });
  });

  it("draws its text with a font that ships with the app", async () => {
    // The test code has no types of the window, so the script is a string.
    // A source that starts with a path has no host, so it is a file of the app.
    const fonts = await browser.execute(`
      return document.fonts.ready.then((faces) => ({
        loadedFamilies: [...faces]
          .filter((face) => face.status === "loaded")
          .map((face) => face.family),
        sourcesOutsideTheApp: [...document.styleSheets]
          .flatMap((sheet) => [...sheet.cssRules])
          .filter((rule) => rule instanceof CSSFontFaceRule)
          .map((rule) => rule.style.getPropertyValue("src"))
          .filter((source) => !source.startsWith('url("/assets/')),
      }));
    `);

    expect(fonts).toEqual({
      loadedFamilies: ["Inter Variable"],
      sourcesOutsideTheApp: [],
    });
  });

  for (const name of [
    "Files",
    "Databases",
    "Events",
    "Engines",
    "User",
    "Settings",
    "Home",
  ]) {
    it(`shows the page ${name} after a click on its entry`, async () => {
      await entry(name).click();

      await expect(heading()).toHaveText(name);
      expect(await currentEntries()).toEqual([name]);

      if (name === "Events") {
        await expect(browser.$("main")).toHaveText(name);
        expect(await browser.$$("main > *").length).toBe(1);
      }
    });
  }

  it("shows the two unconnected chess accounts and their details on User", async () => {
    await entry("User").click();
    expect(await browser.$$("main h2").map((title) => title.getText())).toEqual(
      ["Local profile", "Chess accounts"],
    );
    expect(
      await browser.$$("main h2 + p").map((line) => line.getText()),
    ).toEqual(["On this device", "Account linking is not available yet."]);
    await expect(browser.$$("main summary")).toBeElementsArrayOfSize(2);
    const lichess = browser.$("main details:first-of-type");
    const chessCom = browser.$("main details:nth-of-type(2)");
    for (const [account, name, icon] of [
      [lichess, "Lichess", "lucide-chess-knight"],
      [chessCom, "Chess.com", "lucide-chess-pawn"],
    ] as const) {
      const summary = account.$("summary");
      await expect(summary.$(`span=${name}`)).toHaveText(name);
      await expect(summary.$(`svg.${icon}`)).toBeDisplayed();
      await expect(summary.$("span:last-of-type")).toHaveText("Not connected");
      await expect(account).not.toHaveAttribute("open");
    }
    expect(await screenshot("user")).toEqual({ width: 1280, height: 800 });

    await lichess.$("summary").click();
    await expect(lichess).toHaveAttribute("open");
    await expect(lichess.$("p")).toBeDisplayed();
    await expect(lichess.$("p")).toHaveText(
      "Once linking is available, you can sign in to Lichess from your browser to open your bookmarked games.",
    );
    await lichess.$("summary").click();
    await expect(lichess).not.toHaveAttribute("open");
    await expect(lichess.$("p")).not.toBeDisplayed();
    await expect(lichess.$("summary span:last-of-type")).toHaveText(
      "Not connected",
    );

    nativeKey("tab");
    await expect(chessCom.$("summary")).toBeFocused();
    nativeKey("enter");
    await expect(chessCom).toHaveAttribute("open");
    await expect(chessCom.$("p")).toBeDisplayed();
    await expect(chessCom.$("p")).toHaveText(
      "You can already download your games as PGN from Chess.com and open them in Files.",
    );
    nativeKey("enter");
    await expect(chessCom).not.toHaveAttribute("open");
    await expect(chessCom.$("p")).not.toBeDisplayed();
    await expect(chessCom.$("summary span:last-of-type")).toHaveText(
      "Not connected",
    );
    await expect(heading()).toHaveText("User");
  });

  it("shows on Settings the version that Rust answered", async () => {
    await entry("Settings").click();

    await expect(browser.$("main").$("p")).toHaveText(`zobrist ${version}`);

    expect(await screenshot("settings")).toEqual({ width: 1280, height: 800 });
  });

  it("collapses the icon bar to a strip of icons", async () => {
    await toggle("Collapse the icon bar").click();

    await expect(browser.$("header")).toHaveText("");
    expect(
      await browser.$$("nav button").map((button) => button.getComputedLabel()),
    ).toEqual([
      "Home",
      "Files",
      "Databases",
      "Events",
      "Engines",
      "User",
      "Settings",
    ]);
    expect(await entryNamesAreDisplayed()).toEqual([
      false,
      false,
      false,
      false,
      false,
      false,
      false,
    ]);
    expect(await browser.$("nav").getSize("width")).toBe(48);
    expect(await currentEntries()).toEqual(["Settings"]);
  });

  it("shows pages after clicks on their icons in the collapsed bar", async () => {
    for (const name of ["Databases", "User", "Events"]) {
      await entry(name).click();

      await expect(heading()).toHaveText(name);
      expect(await currentEntries()).toEqual([name]);
    }

    expect(await screenshot("collapsed")).toEqual({ width: 1280, height: 800 });
  });

  it("opens the icon bar again and keeps the current page", async () => {
    await toggle("Open the icon bar").click();

    await expect(browser.$("header")).toHaveText("zobrist");
    expect(await entryNamesAreDisplayed()).toEqual([
      true,
      true,
      true,
      true,
      true,
      true,
      true,
    ]);
    expect(await browser.$("nav").getSize("width")).toBe(208);
    expect(await currentEntries()).toEqual(["Events"]);
  });

  it("opens a tab after each click on the plus button", async () => {
    await openTabAndWait();

    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [true],
    });

    await openTabAndWait();

    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [false, true],
    });
    expect(await tabs().map((tab) => tab.getComputedLabel())).toEqual([
      "New game",
      "New game",
    ]);
    expect(await tab(1).getSize()).toEqual({ width: 160, height: 28 });
    expect(await tab(1).getLocation("x")).toBe(208 + 8);
    expect(await tab(2).getLocation("x")).toBe(208 + 8 + 160 + 4);
    expect(await plusButton().getLocation("x")).toBe(208 + 8 + 2 * (160 + 4));

    expect(await screenshot("tabs")).toEqual({ width: 1280, height: 800 });
  });

  it("shows a board in the start position in a tab", async () => {
    expect(await browser.$$("main cg-board piece").length).toBe(32);
    expect(await drawnBoard()).toEqual(startPositionWithWhiteAtBottom);
    expect(await drawnCoordinates()).toEqual({
      fromLeft: "abcdefgh",
      fromTop: "87654321",
    });
  });

  it("lays out the board, the button under it, and the empty notation area", async () => {
    const notationArea = browser.$("main aside");
    expect(await notationArea.getSize("width")).toBe(320);
    expect(await notationArea.getLocation("x")).toBe(1280 - 320);
    expect(await notationArea.$("ol").getText()).toBe("");

    const board = browser.$("main cg-board");
    expect(await board.getSize()).toEqual({ width: 672, height: 672 });
    expect(await board.getLocation()).toEqual({
      x: 208 + 24 + (1280 - 208 - 320 - 2 * 24 - 672) / 2,
      y: 40 + 24,
    });
    expect(await turnButton().getSize()).toEqual({ width: 32, height: 28 });
    expect(await turnButton().getLocation()).toEqual({
      x: 208 + 24 + (1280 - 208 - 320 - 2 * 24 - 672) / 2,
      y: 40 + 24 + 672 + 12,
    });

    expect(await screenshot("board")).toEqual({ width: 1280, height: 800 });
  });

  it("turns the board of the shown tab and of no other tab", async () => {
    await turnButton().click();

    expect(await drawnBoard()).toEqual(startPositionWithBlackAtBottom);
    expect(await drawnCoordinates()).toEqual({
      fromLeft: "hgfedcba",
      fromTop: "12345678",
    });

    await tab(1).click();

    expect(await drawnBoard()).toEqual(startPositionWithWhiteAtBottom);

    await tab(2).click();

    expect(await drawnBoard()).toEqual(startPositionWithBlackAtBottom);

    await turnButton().click();

    expect(await drawnBoard()).toEqual(startPositionWithWhiteAtBottom);
    expect(await drawnCoordinates()).toEqual({
      fromLeft: "abcdefgh",
      fromTop: "87654321",
    });
  });

  it("leaves a dragged piece on its square", async () => {
    await browser
      .$("main cg-board piece.white.king")
      .dragAndDrop({ x: 0, y: -2 * (672 / 8) });

    await expectSoon(drawnBoard, startPositionWithWhiteAtBottom);
  });

  it("fits the board to the size of the window", async () => {
    await browser.setWindowSize(900, 600);

    const board = browser.$("main cg-board");
    await expect(board).toHaveSize({ width: 320, height: 320 });
    expect(await browser.$("main aside").getSize("width")).toBe(320);
    expect(await drawnBoard()).toEqual(startPositionWithWhiteAtBottom);

    await browser.setWindowSize(1280, 800);

    await expect(board).toHaveSize({ width: 672, height: 672 });
    expect(await browser.$("main aside").getSize("width")).toBe(320);
  });

  it("shows a page again after a click on its entry and keeps the tabs", async () => {
    await entry("Files").click();

    expect(await shown()).toEqual({
      headings: ["Files"],
      boards: 0,
      currentEntries: ["Files"],
      tabsAreSelected: [false, false],
    });
  });

  it("shows a tab again after a click on it", async () => {
    await tab(1).click();

    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [true, false],
    });
  });

  it("closes the shown tab and shows the next tab, and the page after the last tab", async () => {
    await closeButton(1).click();

    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [true],
    });

    await closeButton(1).click();

    expect(await shown()).toEqual({
      headings: ["Files"],
      boards: 0,
      currentEntries: ["Files"],
      tabsAreSelected: [],
    });
  });

  it("keeps the page shown when a tab closes", async () => {
    await openTabAndWait();
    await entry("Home").click();
    await closeButton(1).click();

    expect(await shown()).toEqual({
      headings: ["Home"],
      boards: 0,
      currentEntries: ["Home"],
      tabsAreSelected: [],
    });
  });

  it("keeps the shown tab when another tab closes", async () => {
    await openTabAndWait();
    await openTabAndWait();
    await openTabAndWait();
    await closeButton(1).click();

    expect((await shown()).tabsAreSelected).toEqual([false, true]);
  });

  it("shows the tab at the right of a closed tab, and without one the tab at its left", async () => {
    await openTabAndWait();
    await tab(2).click();
    expect((await shown()).tabsAreSelected).toEqual([false, true, false]);

    await closeButton(2).click();

    expect((await shown()).tabsAreSelected).toEqual([false, true]);

    await closeButton(2).click();

    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [true],
    });
  });

  it("keeps the tabs over the area when the icon bar collapses", async () => {
    await toggle("Collapse the icon bar").click();

    expect(await tab(1).getLocation("x")).toBe(48 + 8);
    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [true],
    });
  });

  it("opens, shows, and closes tabs while the icon bar is collapsed", async () => {
    await openTabAndWait();
    expect((await shown()).tabsAreSelected).toEqual([false, true]);

    await entry("Files").click();
    expect(await shown()).toEqual({
      headings: ["Files"],
      boards: 0,
      currentEntries: ["Files"],
      tabsAreSelected: [false, false],
    });

    await tab(1).click();
    expect(await shown()).toEqual({
      headings: [],
      boards: 1,
      currentEntries: [],
      tabsAreSelected: [true, false],
    });

    await closeButton(1).click();
    expect((await shown()).tabsAreSelected).toEqual([true]);

    await closeButton(1).click();
    expect(await shown()).toEqual({
      headings: ["Files"],
      boards: 0,
      currentEntries: ["Files"],
      tabsAreSelected: [],
    });
    expect(await plusButton().getLocation("x")).toBe(48 + 8);
  });

  it("moves a piece by dragging and by two clicks, and writes the moves", async () => {
    await toggle("Open the icon bar").click();
    await openTabAndWait();

    await dragPiece("e2", "e4");

    await expectSoon(() => game("e2 e4"), {
      pieces: { e2: ".", e4: "P" },
      lastMove: "e2 e4",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4",
      currentMoves: ["e4"],
    });

    await clickSquare("e7");

    await expectSoon(() => game("e7 e5"), {
      pieces: { e7: "p", e5: "." },
      lastMove: "e2 e4",
      targets: "e5 e6",
      selected: "e7",
      check: "",
      notation: "1. e4",
      currentMoves: ["e4"],
    });

    await clickSquare("e5");

    await expectSoon(() => game("e7 e5"), {
      pieces: { e7: ".", e5: "p" },
      lastMove: "e5 e7",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5",
      currentMoves: ["e5"],
    });
  });

  it("puts a piece back after a move that is not legal", async () => {
    await dragPiece("g1", "g3");

    await expectSoon(() => game("g1 g3"), {
      pieces: { g1: "N", g3: "." },
      lastMove: "e5 e7",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5",
      currentMoves: ["e5"],
    });
  });

  it("lets only the side to move pick up a piece", async () => {
    await clickSquare("d7");

    expect(await game("d7")).toEqual({
      pieces: { d7: "p" },
      lastMove: "e5 e7",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5",
      currentMoves: ["e5"],
    });
  });

  it("writes the main line with its move numbers and marks the last move", async () => {
    await playMoves("g1f3");

    await expectSoon(() => game("g1 f3"), {
      pieces: { g1: ".", f3: "N" },
      lastMove: "f3 g1",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3",
      currentMoves: ["Nf3"],
    });
    await expect(
      browser.$$("main cg-board piece.anim"),
    ).toBeElementsArrayOfSize(0);
    expect(await screenshot("moves")).toEqual({ width: 1280, height: 800 });
  });

  it("castles when the king goes two squares", async () => {
    await playMoves("b8c6 f1c4 f8c5");

    await dragPiece("e1", "g1");

    await expectSoon(() => game("e1 f1 g1 h1"), {
      pieces: { e1: ".", f1: "R", g1: "K", h1: "." },
      lastMove: "e1 g1",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. O-O",
      currentMoves: ["O-O"],
    });
  });

  it("takes a pawn in passing", async () => {
    await openTabAndWait();
    await playMoves("e2e4 a7a6 e4e5 d7d5 e5d6");

    await expectSoon(() => game("e5 d5 d6"), {
      pieces: { e5: ".", d5: ".", d6: "P" },
      lastMove: "d6 e5",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 a6 2. e5 d5 3. exd6",
      currentMoves: ["exd6"],
    });
  });

  it("asks for the piece of a pawn that reaches the last row", async () => {
    await openTabAndWait();
    await playMoves("h2h4 g7g5 h4g5 h7h6 g5h6 a7a6 h6h7 a6a5");
    const beforeThePromotion = {
      pieces: { h7: "P", g8: "n" },
      lastMove: "a5 a6",
      targets: "",
      selected: "",
      check: "",
      notation: "1. h4 g5 2. hxg5 h6 3. gxh6 a6 4. h7 a5",
      currentMoves: ["a5"],
    };
    const chooser = () => browser.$("main fieldset");
    const choices = () =>
      chooser()
        .$$("button")
        .map((button) => button.getComputedLabel());

    await dragPiece("h7", "g8");

    await expect(chooser()).toBeDisplayed();
    expect(await chooser().getComputedRole()).toBe("group");
    expect(await choices()).toEqual(["Queen", "Knight", "Rook", "Bishop"]);
    expect(await chooser().getSize()).toEqual({ width: 84, height: 4 * 84 });
    expect(await chooser().getLocation()).toEqual({
      x: 208 + 24 + 16 + 6 * 84,
      y: 40 + 24,
    });
    expect(await chooser().$("aria/Queen").getLocation("y")).toBeLessThan(
      await chooser().$("aria/Bishop").getLocation("y"),
    );

    await browser.keys("Escape");

    await expect(chooser()).not.toBeExisting();
    await expectSoon(() => game("h7 g8"), beforeThePromotion);

    await dragPiece("h7", "g8");
    await expect(chooser()).toBeDisplayed();
    await tab(1).click();
    await tab(3).click();

    await expect(chooser()).not.toBeExisting();
    await expectSoon(() => game("h7 g8"), beforeThePromotion);

    await turnButton().click();
    await dragPiece("h7", "g8", "black");

    await expect(chooser()).toBeDisplayed();
    expect(await chooser().getLocation()).toEqual({
      x: 208 + 24 + 16 + 1 * 84,
      y: 40 + 24 + 4 * 84,
    });
    expect(await chooser().$("aria/Queen").getLocation("y")).toBeGreaterThan(
      await chooser().$("aria/Bishop").getLocation("y"),
    );

    await chooser().$("aria/Knight").click();

    await expect(chooser()).not.toBeExisting();
    await expectSoon(() => game("h7 g8", "black"), {
      pieces: { h7: ".", g8: "N" },
      lastMove: "g8 h7",
      targets: "",
      selected: "",
      check: "",
      notation: "1. h4 g5 2. hxg5 h6 3. gxh6 a6 4. h7 a5 5. hxg8=N",
      currentMoves: ["hxg8=N"],
    });
  });

  it("marks the king in check and takes no move after mate", async () => {
    await openTabAndWait();
    await playMoves("e2e4 e7e5 d1h5 b8c6 f1c4 g8f6 h5f7");
    const afterMate = {
      pieces: { f7: "Q", e8: "k" },
      lastMove: "f7 h5",
      targets: "",
      selected: "",
      check: "e8",
      notation: "1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7#",
      currentMoves: ["Qxf7#"],
    };

    await expectSoon(() => game("f7 e8"), afterMate);

    await clickSquare("e8");
    expect(await game("f7 e8")).toEqual(afterMate);

    await clickSquare("a1");
    expect(await game("f7 e8")).toEqual(afterMate);
  });

  it("keeps the game of each tab and takes a move on a turned board", async () => {
    await tab(1).click();

    expect(await drawnBoard()).toEqual([
      "r.bqk.nr",
      "pppp.ppp",
      "..n.....",
      "..b.p...",
      "..B.P...",
      ".....N..",
      "PPPP.PPP",
      "RNBQ.RK.",
    ]);
    expect(await game("g1")).toEqual({
      pieces: { g1: "K" },
      lastMove: "e1 g1",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. O-O",
      currentMoves: ["O-O"],
    });

    await turnButton().click();

    expect(await drawnBoard()).toEqual([
      ".KR.QBNR",
      "PPP.PPPP",
      "..N.....",
      "...P.B..",
      "...p.b..",
      ".....n..",
      "ppp.pppp",
      "rn.kqb.r",
    ]);

    await playMoves("d7d6", "black");

    await expectSoon(() => game("d7 d6", "black"), {
      pieces: { d7: ".", d6: "p" },
      lastMove: "d6 d7",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. O-O d6",
      currentMoves: ["d6"],
    });
  });

  // The icon bar needs 268 px, so the window is no lower than that and the top bar. Sixty moves
  // fill ten lines, which is more than the notation area holds in that window.
  it("lets a notation that is longer than its area scroll, and keeps the window as it is", async () => {
    await openTabAndWait();
    await playMoves(Array(15).fill("g1f3 g8f6 f3g1 f6g8").join(" "));

    await browser.setWindowSize(1280, 40 + 268);

    await expect(browser.$("main aside")).toHaveSize({
      width: 320,
      height: 268,
    });
    expect(
      await browser.execute(`
        const scrolled = document.querySelector("main aside ol").parentElement;
        const row = document.querySelector("main aside > div");
        scrolled.scrollTop = scrolled.scrollHeight;
        return {
          heightOfThePage: document.documentElement.scrollHeight,
          notationScrolledDown: scrolled.scrollTop > 0,
          topOfTheRowOfButtons: row.getBoundingClientRect().y,
          heightOfTheRowOfButtons: row.getBoundingClientRect().height,
        };
      `),
    ).toEqual({
      heightOfThePage: 40 + 268,
      notationScrolledDown: true,
      topOfTheRowOfButtons: 40,
      heightOfTheRowOfButtons: 40,
    });

    await browser.setWindowSize(1280, 800);

    await expect(browser.$("main cg-board")).toHaveSize({
      width: 672,
      height: 672,
    });
  }).timeout(60_000);

  it("goes to a move after a click on it in the notation", async () => {
    await openTabAndWait();
    await playMoves("e2e4 e7e5 g1f3");

    expect(await moveInNotation("e4").getComputedRole()).toBe("button");
    await moveInNotation("e4").click();

    await expectSoon(() => game("e2 e4 e7 e5 g1 f3"), {
      pieces: { e2: ".", e4: "P", e7: "p", e5: ".", g1: "N", f3: "." },
      lastMove: "e2 e4",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3",
      currentMoves: ["e4"],
    });
  });

  it("walks through the game with the arrow keys, Home, and End", async () => {
    const atTheStart = {
      pieces: { e2: "P", e4: ".", e7: "p", e5: ".", g1: "N", f3: "." },
      lastMove: "",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3",
      currentMoves: [],
    };
    const afterTheLastMove = {
      pieces: { e2: ".", e4: "P", e7: ".", e5: "p", g1: ".", f3: "N" },
      lastMove: "f3 g1",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3",
      currentMoves: ["Nf3"],
    };

    await browser.keys("ArrowRight");

    await expectSoon(() => game("e2 e4 e7 e5 g1 f3"), {
      pieces: { e2: ".", e4: "P", e7: ".", e5: "p", g1: "N", f3: "." },
      lastMove: "e5 e7",
      targets: "",
      selected: "",
      check: "",
      notation: "1. e4 e5 2. Nf3",
      currentMoves: ["e5"],
    });

    await browser.keys("ArrowLeft");
    await expectSoon(markedNotation, "1. [e4] e5 2. Nf3");
    await browser.keys("ArrowLeft");

    await expectSoon(() => game("e2 e4 e7 e5 g1 f3"), atTheStart);

    await browser.keys("ArrowLeft");
    await browser.keys("End");

    await expectSoon(() => game("e2 e4 e7 e5 g1 f3"), afterTheLastMove);

    await browser.keys("ArrowRight");
    await browser.keys("ArrowLeft");

    await expectSoon(markedNotation, "1. e4 [e5] 2. Nf3");

    await browser.keys("Home");

    await expectSoon(() => game("e2 e4 e7 e5 g1 f3"), atTheStart);
  });

  it("starts a variation with a move in the middle of the game", async () => {
    await moveInNotation("e4").click();
    await expectSoon(markedNotation, "1. [e4] e5 2. Nf3");

    await playMoves("c7c5");

    await expectSoon(markedNotation, "1. e4 e5 (1... [c5]) 2. Nf3");

    await playMoves("b1c3");

    await expectSoon(markedNotation, "1. e4 e5 (1... c5 2. [Nc3]) 2. Nf3");
  });

  it("starts a variation inside a variation", async () => {
    await browser.keys("ArrowLeft");
    await expectSoon(markedNotation, "1. e4 e5 (1... [c5] 2. Nc3) 2. Nf3");

    await playMoves("g1f3");

    await expectSoon(
      markedNotation,
      "1. e4 e5 (1... c5 2. Nc3 (2. [Nf3])) 2. Nf3",
    );
    await expect(
      browser.$$("main cg-board piece.anim"),
    ).toBeElementsArrayOfSize(0);
    expect(await screenshot("variations")).toEqual({
      width: 1280,
      height: 800,
    });

    await browser.keys("End");
    await browser.keys("ArrowLeft");

    await expectSoon(
      markedNotation,
      "1. e4 e5 (1... [c5] 2. Nc3 (2. Nf3)) 2. Nf3",
    );
  });

  it("writes a variation of the main line on a line of its own", async () => {
    expect(
      await browser.execute(`
        const notation = document.querySelector("main aside ol").getBoundingClientRect();
        const variation = document.querySelector("main aside ol > li > span + span");
        const innerVariation = variation.querySelector("span + span");
        const isOnTheLineOfItsMove = (written) => {
          const { y } = written.getBoundingClientRect();
          const move = written.previousElementSibling.getBoundingClientRect();
          return y >= move.y && y < move.y + move.height;
        };
        return {
          fromLeft: variation.getBoundingClientRect().x - notation.x,
          isOnTheLineOfItsMove: isOnTheLineOfItsMove(variation),
          innerVariationIsOnTheLineOfItsMove: isOnTheLineOfItsMove(innerVariation),
        };
      `),
    ).toEqual({
      fromLeft: 16,
      isOnTheLineOfItsMove: false,
      innerVariationIsOnTheLineOfItsMove: true,
    });
  });

  it("goes to a move that the game has when that move is played again", async () => {
    await moveInNotation("e4").click();
    await expectSoon(
      markedNotation,
      "1. [e4] e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );

    await clickSquare("e7");
    await clickSquare("e5");

    await expectSoon(
      markedNotation,
      "1. e4 [e5] (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );
  });

  it("starts a variation of the first move", async () => {
    await browser.keys("Home");
    await expectSoon(
      markedNotation,
      "1. e4 e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );

    await playMoves("d2d4");

    await expectSoon(
      markedNotation,
      "1. e4 (1. [d4]) 1... e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );
  });

  it("takes the keys only while its tab is shown", async () => {
    await entry("Files").click();
    await browser.keys("ArrowLeft");
    await tab(6).click();

    await expectSoon(
      markedNotation,
      "1. e4 (1. [d4]) 1... e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );

    await browser.keys("ArrowLeft");

    await expectSoon(
      markedNotation,
      "1. e4 (1. d4) 1... e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );
  });

  it("leaves the keys to a field for text that has the focus", async () => {
    await browser.keys("Home");
    await expectSoon(
      markedNotation,
      "1. e4 (1. d4) 1... e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );
    await browser.execute(`
      const field = document.createElement("input");
      document.body.append(field);
      field.focus();
    `);

    await browser.keys("End");

    await browser.execute(`document.querySelector("body > input").remove();`);
    await browser.keys("ArrowRight");

    await expectSoon(
      markedNotation,
      "1. [e4] (1. d4) 1... e5 (1... c5 2. Nc3 (2. Nf3)) 2. Nf3",
    );
  });

  it("drops a waiting promotion when the game goes to another move", async () => {
    await tab(3).click();
    await browser.keys("ArrowLeft");
    await expectSoon(
      markedNotation,
      "1. h4 g5 2. hxg5 h6 3. gxh6 a6 4. h7 [a5] 5. hxg8=N",
    );
    await dragPiece("h7", "g8", "black");
    await expect(browser.$("main fieldset")).toBeDisplayed();

    await browser.keys("ArrowLeft");

    await expect(browser.$("main fieldset")).not.toBeExisting();
    await expectSoon(
      markedNotation,
      "1. h4 g5 2. hxg5 h6 3. gxh6 a6 4. [h7] a5 5. hxg8=N",
    );
  });

  it("starts a key from the move that the key before it reached, also in the same moment", async () => {
    // The second key comes at once when the notation marks the move of the first key.
    await browser.execute(`
      const pressLeftArrow = () =>
        document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
      new MutationObserver((changes, observer) => {
        observer.disconnect();
        pressLeftArrow();
      }).observe(document.querySelector("main aside"), {
        attributes: true,
        subtree: true,
        attributeFilter: ["aria-current"],
      });
      pressLeftArrow();
    `);

    await expectSoon(
      markedNotation,
      "1. h4 g5 2. hxg5 h6 3. [gxh6] a6 4. h7 a5 5. hxg8=N",
    );
  });
});
