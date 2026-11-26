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

const entry = (name: string) => browser.$("nav").$(`button=${name}`);
const currentEntries = () =>
  browser
    .$$('nav button[aria-current="page"]')
    .map((button) => button.getText());
const heading = () => browser.$("main").$("h1");

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
    ).toEqual(["Home", "Files", "Databases", "Engines", "Settings"]);
    expect(await currentEntries()).toEqual(["Home"]);

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

  for (const name of ["Files", "Databases", "Engines", "Settings", "Home"]) {
    it(`shows the page ${name} after a click on its entry`, async () => {
      await entry(name).click();

      await expect(heading()).toHaveText(name);
      expect(await currentEntries()).toEqual([name]);
    });
  }

  it("shows on Settings the version that Rust answered", async () => {
    await entry("Settings").click();

    await expect(browser.$("main").$("p")).toHaveText(`zobrist ${version}`);

    expect(await screenshot("settings")).toEqual({ width: 1280, height: 800 });
  });
});
