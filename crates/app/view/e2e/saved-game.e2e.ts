import {
  copyFileSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
// The test changes files only in this folder.
const work = `${screenshots}/saved-game`;
const fixture = fileURLToPath(
  new URL("../../../cli/tests/fixtures/clean.pgn", import.meta.url),
);
const file = `${work}/clean.pgn`;
const fileText = () => readFileSync(file, "utf8");

const board = () => browser.$("main cg-board");
const tab = (positionFromLeft: number) =>
  browser.$(
    `header [role="tablist"] > :nth-child(${positionFromLeft}) [role="tab"]`,
  );
const tabs = () => browser.$$('header [role="tab"]');
const dot = (positionFromLeft: number) => tab(positionFromLeft).$("span:empty");
const closeButton = (positionFromLeft: number) =>
  browser.$(
    `header [role="tablist"] > :nth-child(${positionFromLeft}) > [aria-label^="Close "]`,
  );
const tool = (name: string) => browser.$("main aside").$(`aria/${name}`);
const refusedSave = () => browser.$('main aside [role="alert"]');
const form = () => browser.$("main aside form");
const commentField = () => browser.$("main aside").$("aria/Comment");

/** The notation as text, with the current move in brackets. */
async function notation() {
  // The test code has no types of the window, so the script is a string and its result has no type.
  const text: unknown = await browser.execute(`
    const texts = document.createTreeWalker(document.querySelector("main aside ol"), NodeFilter.SHOW_TEXT);
    const parts = [];
    while (texts.nextNode()) {
      const text = texts.currentNode.textContent;
      const isCurrent = texts.currentNode.parentElement.hasAttribute("aria-current");
      parts.push(isCurrent ? "[" + text + "]" : text);
    }
    return parts.join(" ");
  `);
  return text;
}

/** Where the middle of a square lies from the middle of the board, with White at the bottom. */
async function fromMiddleOfBoard(square: string) {
  const squareSide = (await board().getSize("width")) / 8;
  return {
    x: ("abcdefgh".indexOf(square.charAt(0)) - 3.5) * squareSide,
    y: (4.5 - Number(square.charAt(1))) * squareSide,
  };
}

async function playMove(from: string, to: string) {
  const movesBefore = await browser.$$("main aside li").length;
  await board().click(await fromMiddleOfBoard(from));
  await board().click(await fromMiddleOfBoard(to));
  await browser.waitUntil(
    async () => (await browser.$$("main aside li").length) === movesBefore + 1,
    { timeoutMsg: `the notation did not get the move ${from}${to}` },
  );
}

async function showFilesPage() {
  await browser.$("nav").$("aria/Files").click();
  await expect(browser.$("main h1")).toHaveText("Files");
}

const rows = () => browser.$$("main table tbody tr");
async function listedGames() {
  const texts: string[][] = [];
  for (const row of await rows().getElements()) {
    texts.push(await row.$$("td").map((cell) => cell.getText()));
  }
  return texts;
}

/** The text of every field of the form and of its choice. */
async function shownData() {
  const result: unknown = await browser.execute(`
    return document.querySelector("main aside form select").selectedOptions[0].textContent;
  `);
  const texts: Record<string, unknown> = { Result: result };
  for (const name of ["White", "Black", "Event", "Site", "Date", "Round"]) {
    texts[name] = await form().$(`aria/${name}`).getValue();
  }
  return texts;
}

const firstGame = `[Event "Example"]
[Site "?"]
[Date "2024.01.01"]
[Round "1"]
[White "Alpha"]
[Black "Beta"]
[Result "1-0"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6?? {Overlooks the mate.} (3... g6 4. Qf3) 4. Qxf7# 1-0
`;
const secondGame = (white: string) => `[Event "Example"]
[Site "?"]
[Date "2024.01.01"]
[Round "2"]
[White "${white}"]
[Black "Delta"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# {Fool's mate} 0-1
`;
const newGame = `1. e4 e5 *
`;
// The first game as zobrist writes it, with the variation that the test plays.
const firstGameWithVariation = `[Event "Example"]
[Site "?"]
[Date "2024.01.01"]
[Round "1"]
[White "Alpha"]
[Black "Beta"]
[Result "1-0"]

1. e4 (1. d4) 1... e5 2. Qh5 Nc6 3. Bc4 Nf6 $4 {Overlooks the mate.} (3... g6 4. Qf3) 4. Qxf7# 1-0
`;

describe("the games of a file in tabs", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    rmSync(work, { recursive: true, force: true });
    mkdirSync(work, { recursive: true });
    copyFileSync(fixture, file);
  });

  it("asks for a file on the page Files before it saves a new game", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(board()).toExist();
    expect(
      await browser
        .$$("main aside > div:first-child button")
        .map((button) => button.getComputedLabel()),
    ).toEqual([
      "Save",
      "Save as new game",
      "Undo",
      "Redo",
      "Promote variation",
      "Delete variation",
      "Delete the rest",
    ]);
    expect(await tool("Save").isEnabled()).toBe(false);
    expect(await tool("Save as new game").isEnabled()).toBe(false);
    await expect(dot(1)).not.toExist();

    // The two buttons stand at the left end of the row, with 12 px behind them.
    const saveLeft = await tool("Save").getLocation("x");
    expect(saveLeft).toBe(960 + 1 + 8);
    expect(await tool("Save as new game").getLocation("x")).toBe(
      saveLeft + 32 + 2,
    );
    expect(await tool("Undo").getLocation("x")).toBe(
      saveLeft + 2 * 32 + 2 + 12,
    );

    await playMove("e2", "e4");

    await expect(tab(1)).toHaveAttribute("aria-label", "New game, not saved");
    await expect(tab(1)).toHaveText("New game");
    expect(await tool("Save").isEnabled()).toBe(true);
    expect(await tool("Save as new game").isEnabled()).toBe(true);

    await tool("Save").click();

    await expect(refusedSave()).toHaveText(
      "Open a file on the page Files first.",
    );
    expect(await refusedSave().getLocation()).toEqual({ x: 961, y: 40 + 40 });
    expect((await refusedSave().getCSSProperty("font-size")).value).toBe(
      "12px",
    );
    expect((await refusedSave().getCSSProperty("color")).parsed.hex).toBe(
      "#b3261e",
    );
    expect((await refusedSave().getCSSProperty("padding")).value).toBe(
      "6px 16px",
    );
    await expect(dot(1)).toExist();
    expect(fileText()).toBe(readFileSync(fixture, "utf8"));

    // The line goes with the next change of the game.
    await playMove("e7", "e5");
    await expect(refusedSave()).not.toExist();
  });

  it("opens a game of the list in a tab", async () => {
    await showFilesPage();
    await browser.$("main").$("aria/Path of a PGN file").setValue(file);
    await browser.$("main").$("button=Open").click();
    await expect(rows()).toBeElementsArrayOfSize(2);

    await rows()[1]?.click();

    await expect(tab(2)).toHaveAttribute("aria-selected", "true");
    await expect(tab(2)).toHaveText("Gamma - Delta");
    expect(await tab(2).getAttribute("aria-label")).toBeNull();
    await expect(dot(2)).not.toExist();
    expect(await notation()).toBe("1. f3 e5 2. g4 Qh4#");
    expect(await browser.$$("main cg-board piece").length).toBe(32);
    expect(await browser.$$("main cg-board square.last-move").length).toBe(0);
    expect(await shownData()).toEqual({
      White: "Gamma",
      Black: "Delta",
      Event: "Example",
      Site: "?",
      Date: "2024.01.01",
      Round: "2",
      Result: "0-1",
    });
    expect(await tool("Save").isEnabled()).toBe(false);
    expect(await tool("Save as new game").isEnabled()).toBe(true);
  });

  it("shows the tab of a game that is open already", async () => {
    await showFilesPage();
    await rows()[1]?.click();

    await expect(tab(2)).toHaveAttribute("aria-selected", "true");
    await expect(tabs()).toBeElementsArrayOfSize(2);
  });

  it("shows a dot while the game has changes, and Save writes the game into its place", async () => {
    await browser.keys("End");
    await browser.waitUntil(
      async () => (await notation()) === "1. f3 e5 2. g4 [Qh4#]",
    );
    await commentField().setValue("Fool's mate");
    await board().click(await fromMiddleOfBoard("a5"));

    await expect(dot(2)).toExist();
    await expect(tab(2)).toHaveAttribute(
      "aria-label",
      "Gamma - Delta, not saved",
    );
    await expect(tab(2)).toHaveText("Gamma - Delta");
    expect(await tool("Save").isEnabled()).toBe(true);
    expect(await dot(2).getSize()).toEqual({ width: 6, height: 6 });
    expect((await dot(2).getCSSProperty("background-color")).parsed.hex).toBe(
      "#444444",
    );
    const fromDotToName: unknown = await browser.execute(`
      const dot = document.querySelector('header [role="tab"][aria-selected="true"] span');
      const name = document.createRange();
      name.selectNodeContents(dot.nextSibling);
      return name.getBoundingClientRect().x - dot.getBoundingClientRect().right;
    `);
    expect(fromDotToName).toBeCloseTo(6);
    expect(fileText()).toBe(readFileSync(fixture, "utf8"));

    // A click of the driver on a row of the list leaves the focus on the entry of the icon
    // bar, where the click of a player takes it away. The picture shows what a player sees.
    await browser.execute("document.activeElement.blur();");
    const picture = await browser.saveScreenshot(
      `${screenshots}/saved-game.png`,
    );
    // A PNG file holds its width and its height at these two places.
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });

    await tool("Save").click();

    await expect(dot(2)).not.toExist();
    expect(await tab(2).getAttribute("aria-label")).toBeNull();
    expect(await tool("Save").isEnabled()).toBe(false);
    expect(fileText()).toBe(`${firstGame}\n${secondGame("Gamma")}`);
    expect(fileText().split("\n").slice(0, 9)).toEqual(
      readFileSync(fixture, "utf8").split("\n").slice(0, 9),
    );
  });

  it("saves with Ctrl+S what a field of the form holds, and the list shows it", async () => {
    await form().$("aria/White").setValue("Epsilon");
    await browser.keys(["Control", "s"]);

    await expect(tab(2)).toHaveText("Epsilon - Delta");
    await browser.waitUntil(
      async () => fileText() === `${firstGame}\n${secondGame("Epsilon")}`,
      { timeoutMsg: "the file did not get the name that the field held" },
    );
    await expect(dot(2)).not.toExist();

    await showFilesPage();
    expect(await listedGames()).toEqual([
      ["1", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
      ["2", "Epsilon", "Delta", "0-1", "2024.01.01", "Example"],
    ]);
  });

  it("adds a new game to the file of the page Files as its last game", async () => {
    await tab(1).click();
    await expect(dot(1)).toExist();

    await tool("Save").click();

    await expect(dot(1)).not.toExist();
    await expect(refusedSave()).not.toExist();
    expect(fileText()).toBe(
      `${firstGame}\n${secondGame("Epsilon")}\n${newGame}`,
    );

    await showFilesPage();
    expect(await listedGames()).toEqual([
      ["1", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
      ["2", "Epsilon", "Delta", "0-1", "2024.01.01", "Example"],
      ["3", "?", "?", "*", "?", "?"],
    ]);
  });

  it("adds a game of a file once more with Save as new game and keeps the game as it was", async () => {
    await tab(2).click();
    await expect(tab(2)).toHaveAttribute("aria-selected", "true");

    await tool("Save as new game").click();

    await browser.waitUntil(
      async () =>
        fileText() ===
        `${firstGame}\n${secondGame("Epsilon")}\n${newGame}\n${secondGame("Epsilon")}`,
      { timeoutMsg: "the file did not get the game as its last game" },
    );
    await expect(dot(2)).not.toExist();

    await showFilesPage();
    expect(await listedGames()).toEqual([
      ["1", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
      ["2", "Epsilon", "Delta", "0-1", "2024.01.01", "Example"],
      ["3", "?", "?", "*", "?", "?"],
      ["4", "Epsilon", "Delta", "0-1", "2024.01.01", "Example"],
    ]);
  });

  it("keeps every byte of a file that another program changed, and says what to do", async () => {
    await rows()[0]?.click();
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
    await expect(tab(3)).toHaveText("Alpha - Beta");

    // Another program puts a game before all others.
    const changedOutside = `[White "Outside"]\n\n1. d4 *\n\n${fileText()}`;
    writeFileSync(file, changedOutside);
    await playMove("d2", "d4");
    await expect(dot(3)).toExist();

    await tool("Save").click();

    await expect(refusedSave()).toHaveText(
      "The file changed since this game was opened. Save it as a new game.",
    );
    await expect(dot(3)).toExist();
    expect(fileText()).toBe(changedOutside);

    await tool("Save as new game").click();

    await expect(refusedSave()).not.toExist();
    await expect(dot(3)).not.toExist();
    expect(fileText()).toBe(`${changedOutside}\n${firstGameWithVariation}`);
    await showFilesPage();
    expect(await listedGames()).toEqual([
      ["1", "Outside", "?", "*", "?", "?"],
      ["2", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
      ["3", "Epsilon", "Delta", "0-1", "2024.01.01", "Example"],
      ["4", "?", "?", "*", "?", "?"],
      ["5", "Epsilon", "Delta", "0-1", "2024.01.01", "Example"],
      ["6", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
    ]);
  });

  it("asks before it closes a tab whose game has changes", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(4)).toHaveAttribute("aria-selected", "true");
    await playMove("e2", "e4");
    await expect(dot(4)).toExist();
    expect(await tab(4).getSize()).toEqual({ width: 160, height: 28 });

    await closeButton(4).click();

    await expect(tabs()).toBeElementsArrayOfSize(4);
    await expect(tab(4)).toHaveText("Close without saving?");
    await expect(tab(4)).toHaveAttribute("aria-label", "New game, not saved");
    expect(await tab(4).getSize()).toEqual({ width: 280, height: 28 });
    expect((await tab(4).getCSSProperty("font-size")).value).toBe("12px");
    await expect(closeButton(4)).not.toExist();
    const close = browser.$("header").$("aria/Close");
    const keep = browser.$("header").$("aria/Keep");
    await expect(close).toHaveText("Close");
    await expect(keep).toHaveText("Keep");
    expect(await close.getSize("height")).toBe(20);
    expect(await keep.getSize("height")).toBe(20);
    // The question ends before the two buttons, and they stand inside the tab.
    const question = tab(4).$("span:not(:empty)");
    const questionRight =
      (await question.getLocation("x")) + (await question.getSize("width"));
    expect(questionRight).toBeLessThan(await close.getLocation("x"));
    const tabRight = (await tab(4).getLocation("x")) + 280;
    const keepRight =
      (await keep.getLocation("x")) + (await keep.getSize("width"));
    expect(tabRight - keepRight).toBe(4);

    await keep.click();

    await expect(tab(4)).toHaveText("New game");
    expect(await tab(4).getSize("width")).toBe(160);
    await expect(dot(4)).toExist();

    await closeButton(4).click();
    await browser.$("header").$("aria/Close").click();

    await expect(tabs()).toBeElementsArrayOfSize(3);
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
  });

  it("closes a tab without changes at once", async () => {
    await expect(dot(1)).not.toExist();

    await closeButton(1).click();

    await expect(tabs()).toBeElementsArrayOfSize(2);
    await expect(tab(1)).toHaveText("Epsilon - Delta");
  });

  it("asks before it closes a tab whose last change is still on its way to Rust", async () => {
    await tab(1).click();
    await expect(tab(1)).toHaveAttribute("aria-selected", "true");
    await expect(dot(1)).not.toExist();
    await form().$("aria/Site").setValue("Amsterdam");

    // The field sends its text when the mouse goes down on the close button. The driver of
    // the test is too slow to click before Rust answers that text, so the script does both
    // in one go.
    await browser.execute(`
      const close = document.querySelector(
        'header [role="tablist"] > :nth-child(1) > [aria-label^="Close "]',
      );
      close.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      close.click();
    `);

    await expect(tab(1)).toHaveText("Close without saving?");
    await expect(tabs()).toBeElementsArrayOfSize(2);
    await browser.$("header").$("aria/Keep").click();
    await expect(tab(1)).toHaveText("Epsilon - Delta");
    await expect(dot(1)).toExist();
  });

  it("ends the question when the game is saved while the tab asks", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
    await playMove("d2", "d4");
    await closeButton(3).click();
    await expect(tab(3)).toHaveText("Close without saving?");

    await browser.keys(["Control", "s"]);

    await expect(tab(3)).toHaveText("New game");
    await expect(dot(3)).not.toExist();
    await closeButton(3).click();
    await expect(tabs()).toBeElementsArrayOfSize(2);
  });

  it("reads the file again when a game of the list does not open", async () => {
    await showFilesPage();
    await expect(rows()).toBeElementsArrayOfSize(7);
    // Another program leaves one game in the file.
    writeFileSync(file, firstGame);

    await rows()[6]?.click();

    await expect(rows()).toBeElementsArrayOfSize(1);
    expect(await listedGames()).toEqual([
      ["1", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
    ]);
    await expect(tabs()).toBeElementsArrayOfSize(2);
  });

  it("saves with Ctrl+S a comment that is typed and not sent", async () => {
    await browser.$("header").$("aria/New board").click();
    await expect(tab(3)).toHaveAttribute("aria-selected", "true");
    await playMove("e2", "e4");
    await commentField().setValue("King's pawn");

    await browser.keys(["Control", "s"]);

    await browser.waitUntil(
      async () => fileText() === `${firstGame}\n1. e4 {King's pawn} *\n`,
      { timeoutMsg: "the file did not get the comment that the field held" },
    );
    await expect(dot(3)).not.toExist();
    await expect(commentField()).toHaveValue("King's pawn");
  });
});
