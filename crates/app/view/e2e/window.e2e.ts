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

const shown = async () => ({
  heading: await heading().getText(),
  currentEntries: await currentEntries(),
  tabsAreSelected: await tabs().map(
    async (tab) => (await tab.getAttribute("aria-selected")) === "true",
  ),
});

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
    await plusButton().click();

    expect(await shown()).toEqual({
      heading: "New game",
      currentEntries: [],
      tabsAreSelected: [true],
    });
    await expect(browser.$("main")).toHaveText("New game");
    expect(await browser.$$("main > *").length).toBe(1);

    await plusButton().click();

    expect(await shown()).toEqual({
      heading: "New game",
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

  it("shows a page again after a click on its entry and keeps the tabs", async () => {
    await entry("Files").click();

    expect(await shown()).toEqual({
      heading: "Files",
      currentEntries: ["Files"],
      tabsAreSelected: [false, false],
    });
  });

  it("shows a tab again after a click on it", async () => {
    await tab(1).click();

    expect(await shown()).toEqual({
      heading: "New game",
      currentEntries: [],
      tabsAreSelected: [true, false],
    });
  });

  it("closes the shown tab and shows the next tab, and the page after the last tab", async () => {
    await closeButton(1).click();

    expect(await shown()).toEqual({
      heading: "New game",
      currentEntries: [],
      tabsAreSelected: [true],
    });

    await closeButton(1).click();

    expect(await shown()).toEqual({
      heading: "Files",
      currentEntries: ["Files"],
      tabsAreSelected: [],
    });
  });

  it("keeps the page shown when a tab closes", async () => {
    await plusButton().click();
    await entry("Home").click();
    await closeButton(1).click();

    expect(await shown()).toEqual({
      heading: "Home",
      currentEntries: ["Home"],
      tabsAreSelected: [],
    });
  });

  it("keeps the shown tab when another tab closes", async () => {
    await plusButton().click();
    await plusButton().click();
    await plusButton().click();
    await closeButton(1).click();

    expect((await shown()).tabsAreSelected).toEqual([false, true]);
  });

  it("shows the tab at the right of a closed tab, and without one the tab at its left", async () => {
    await plusButton().click();
    await tab(2).click();
    expect((await shown()).tabsAreSelected).toEqual([false, true, false]);

    await closeButton(2).click();

    expect((await shown()).tabsAreSelected).toEqual([false, true]);

    await closeButton(2).click();

    expect(await shown()).toEqual({
      heading: "New game",
      currentEntries: [],
      tabsAreSelected: [true],
    });
  });

  it("keeps the tabs over the area when the icon bar collapses", async () => {
    await toggle("Collapse the icon bar").click();

    expect(await tab(1).getLocation("x")).toBe(48 + 8);
    expect(await shown()).toEqual({
      heading: "New game",
      currentEntries: [],
      tabsAreSelected: [true],
    });
  });

  it("opens, shows, and closes tabs while the icon bar is collapsed", async () => {
    await plusButton().click();
    expect((await shown()).tabsAreSelected).toEqual([false, true]);

    await entry("Files").click();
    expect(await shown()).toEqual({
      heading: "Files",
      currentEntries: ["Files"],
      tabsAreSelected: [false, false],
    });

    await tab(1).click();
    expect(await shown()).toEqual({
      heading: "New game",
      currentEntries: [],
      tabsAreSelected: [true, false],
    });

    await closeButton(1).click();
    expect((await shown()).tabsAreSelected).toEqual([true]);

    await closeButton(1).click();
    expect(await shown()).toEqual({
      heading: "Files",
      currentEntries: ["Files"],
      tabsAreSelected: [],
    });
    expect(await plusButton().getLocation("x")).toBe(48 + 8);
  });
});
