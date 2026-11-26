import { mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";

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

const drawnBoard = () =>
  browser.execute(`
    const board = document.querySelector("main cg-board").getBoundingClientRect();
    const letters = { pawn: "p", knight: "n", bishop: "b", rook: "r", queen: "q", king: "k" };
    const rows = Array.from({ length: 8 }, () => Array(8).fill("."));
    for (const piece of document.querySelectorAll("main cg-board piece")) {
      const { x, y } = piece.getBoundingClientRect();
      const letter = letters[piece.classList[1]];
      const row = Math.round((y - board.y) / (board.height / 8));
      const column = Math.round((x - board.x) / (board.width / 8));
      rows[row][column] = piece.classList[0] === "white" ? letter.toUpperCase() : letter;
    }
    return rows.map((row) => row.join(""));
  `);
const startPositionWithWhiteAtBottom = [
  "rnbqkbnr",
  "pppppppp",
  "........",
  "........",
  "........",
  "........",
  "PPPPPPPP",
  "RNBQKBNR",
];
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

      if (name === "Events" || name === "User") {
        await expect(browser.$("main")).toHaveText(name);
        expect(await browser.$$("main > *").length).toBe(1);
      }
    });
  }

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
    expect(await notationArea.getText()).toBe("");

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

    expect(await drawnBoard()).toEqual(startPositionWithWhiteAtBottom);
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
});
