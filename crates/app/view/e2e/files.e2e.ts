import { execFileSync } from "node:child_process";
import {
  closeSync,
  constants,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
// The test creates files only in this folder.
const work = `${screenshots}/files`;
const fixtures = fileURLToPath(
  new URL("../../../cli/tests/fixtures", import.meta.url),
);
const clean = `${fixtures}/clean.pgn`;
const cleanRows = [
  ["1", "Alpha", "Beta", "1-0", "2024.01.01", "Example"],
  ["2", "Gamma", "Delta", "0-1", "2024.01.01", "Example"],
];

const main = () => browser.$("main");
const heading = () => main().$("h1");
const field = () => main().$("aria/Path of a PGN file");
const button = (name: string) => main().$(`button=${name}`);
const line = () => main().$('[role="status"]');
const list = () => main().$("table");
const headCells = () => list().$$("thead th");
const rows = () => list().$$("tbody tr");
async function cellTexts() {
  const texts: string[][] = [];
  for (const row of await rows().getElements()) {
    texts.push(await row.$$("td").map((cell) => cell.getText()));
  }
  return texts;
}
const lineColor = async () => (await line().getCSSProperty("color")).parsed.hex;

async function open(path: string) {
  await field().setValue(path);
  await button("Open").click();
}

describe("the page Files", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    rmSync(work, { recursive: true, force: true });
    mkdirSync(work, { recursive: true });
  });

  it("shows the field and the two buttons, and no list", async () => {
    await browser.$("nav").$("aria/Files").click();

    await expect(heading()).toHaveText("Files");
    await expect(field()).toHaveValue("");
    expect(
      await main()
        .$$("button")
        .map((each) => each.getText()),
    ).toEqual(["Open", "Create"]);
    await expect(line()).not.toExist();
    await expect(list()).not.toExist();

    expect(await field().getSize()).toEqual({ width: 480, height: 28 });
    expect(await button("Open").getSize("height")).toBe(28);
    expect(await button("Create").getSize("height")).toBe(28);
    const fieldRight = (await field().getLocation("x")) + 480;
    const openLeft = await button("Open").getLocation("x");
    expect(openLeft - fieldRight).toBe(8);
    const openRight = openLeft + (await button("Open").getSize("width"));
    expect((await button("Create").getLocation("x")) - openRight).toBe(8);
    const headingBottom =
      (await heading().getLocation("y")) + (await heading().getSize("height"));
    expect((await field().getLocation("y")) - headingBottom).toBeCloseTo(16);
  });

  it("lists the games of a file after a click on Open", async () => {
    await open(clean);

    await expect(rows()).toBeElementsArrayOfSize(2);
    expect(await headCells().map((cell) => cell.getText())).toEqual([
      "#",
      "White",
      "Black",
      "Result",
      "Date",
      "Event",
    ]);
    expect(await cellTexts()).toEqual(cleanRows);
    await expect(line()).not.toExist();

    // The area is the window without the icon bar and its own padding at both sides.
    expect(await list().getSize("width")).toBe(1280 - 208 - 32 - 40);
    expect(await headCells().map((cell) => cell.getSize("width"))).toEqual([
      48,
      expect.closeTo(776 / 3, 0),
      expect.closeTo(776 / 3, 0),
      72,
      104,
      expect.closeTo(776 / 3, 0),
    ]);
    expect(await rows().map((row) => row.getSize("height"))).toEqual([28, 28]);
    const rowBottom = (await field().getLocation("y")) + 28;
    expect((await list().getLocation("y")) - rowBottom).toBeCloseTo(16);
  });

  it("lists the games of a file after the Enter key in the field", async () => {
    await field().setValue(`${fixtures}/damaged.pgn`);
    await browser.keys("Enter");

    await expect(rows()).toBeElementsArrayOfSize(3);
    expect(await cellTexts()).toEqual([
      ["1", "Alpha", "Beta", "1-0", "?", "Example"],
      ["2", "Gamma", "Delta", "*", "?", "Example"],
      ["3", "Epsilon", "Zeta", "0-1", "?", "Example"],
    ]);
    await expect(line()).toHaveText("1 game has a problem.");
    expect(await lineColor()).toBe("#6b6b6b");
    const rowBottom = (await field().getLocation("y")) + 28;
    const lineTop = await line().getLocation("y");
    expect(lineTop - rowBottom).toBeCloseTo(8);
    const lineBottom = lineTop + (await line().getSize("height"));
    expect((await list().getLocation("y")) - lineBottom).toBeCloseTo(16);
  });

  it("says why a file cannot be opened and shows no list", async () => {
    await open(`${work}/missing.pgn`);

    await expect(line()).toHaveText(
      "This file cannot be opened. No such file or directory (os error 2)",
    );
    expect(await lineColor()).toBe("#b3261e");
    await expect(list()).not.toExist();
  });

  it("creates an empty file and never replaces a file", async () => {
    const created = `${work}/new.pgn`;
    await field().setValue(created);
    await button("Create").click();

    await expect(line()).toHaveText("The file has no games.");
    expect(await lineColor()).toBe("#6b6b6b");
    await expect(list()).not.toExist();
    expect(readFileSync(created, "utf8")).toBe("");

    writeFileSync(created, "1. e4 *\n");
    await button("Create").click();

    await expect(line()).toHaveText("This file exists already.");
    expect(await lineColor()).toBe("#b3261e");
    expect(readFileSync(created, "utf8")).toBe("1. e4 *\n");
  });

  it("says why a file cannot be created", async () => {
    await field().setValue(`${work}/missing-folder/new.pgn`);
    await button("Create").click();

    await expect(line()).toHaveText(
      "This file cannot be created. No such file or directory (os error 2)",
    );
  });

  it("shows an answer that arrives after the page was left and shown again", async () => {
    // Rust waits at a pipe until somebody writes into it, so the test decides when it answers.
    const slow = `${work}/slow.pgn`;
    execFileSync("mkfifo", [slow]);
    await open(slow);
    await browser.$("nav").$("aria/Home").click();
    await expect(heading()).toHaveText("Home");
    await browser.$("nav").$("aria/Files").click();
    await expect(field()).toHaveValue(slow);

    // Without a reader this open fails, and with a blocking open the test could hang.
    const pipe = await browser.waitUntil(async () => {
      try {
        return openSync(slow, constants.O_WRONLY | constants.O_NONBLOCK);
      } catch {
        return false;
      }
    });
    writeSync(pipe, '[White "Late"]\n\n1. e4 *\n');
    closeSync(pipe);

    await expect(rows()).toBeElementsArrayOfSize(1);
    expect(await cellTexts()).toEqual([["1", "Late", "?", "*", "?", "?"]]);
  });

  it("keeps its file while another page and a tab are shown", async () => {
    await open(clean);
    await expect(rows()).toBeElementsArrayOfSize(2);

    await browser.$("nav").$("aria/Home").click();
    await expect(heading()).toHaveText("Home");
    await browser.$("header").$("aria/New board").click();
    await expect(browser.$('header [role="tab"]')).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(heading()).not.toExist();
    await browser.$("nav").$("aria/Files").click();

    await expect(heading()).toHaveText("Files");
    await expect(field()).toHaveValue(clean);
    expect(await cellTexts()).toEqual(cleanRows);

    const picture = await browser.saveScreenshot(`${screenshots}/files.png`);
    // A PNG file holds its width and its height at these two places.
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });
  });
});
