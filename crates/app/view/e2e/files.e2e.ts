import { execFileSync, spawnSync } from "node:child_process";
import {
  closeSync,
  constants,
  copyFileSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
  writeFileSync,
  writeSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";
import { pick } from "./file-picker";
import { exportOrigin, exportPaths, replyWithPlayerGames } from "./game-export";

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
const selection = () => main().$('section[aria-label="Selection"]');
const pathLine = (path: string) => selection().$(`p=${path}`);
const count = (text: string) => selection().$(`span=${text}`);
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
const tabs = () => browser.$$('header [role="tab"]');

async function open(path: string) {
  await pick("File", path);
}

/**
 * Opens a pipe on the page. Rust waits at the pipe until the returned function writes a game
 * into it, so the test decides when Rust answers. Returns when Rust waits.
 */
async function openHeldBack(path: string) {
  execFileSync("mkfifo", [path]);
  await open(path);
  // Without a reader this open fails, and with a blocking open the test could hang.
  const pipe = await browser.waitUntil(async () => {
    try {
      return openSync(path, constants.O_WRONLY | constants.O_NONBLOCK);
    } catch {
      return false;
    }
  });
  return () => {
    writeSync(pipe, '[White "Late"]\n\n1. e4 *\n');
    closeSync(pipe);
  };
}

describe("the page Files", () => {
  before(async () => {
    // Before each command the service asks a second plugin which window has the focus, and
    // the app does not carry that plugin. A window that the test chose itself turns that off.
    await browser.switchToWindow("main");
    rmSync(work, { recursive: true, force: true });
    mkdirSync(work, { recursive: true });
  });

  it("offers Open and New PGN and has no selected file", async () => {
    await browser.$("nav").$("aria/Files").click();

    await expect(heading()).toHaveText("Files");
    expect(
      await main()
        .$$("button")
        .map((each) => each.getText()),
    ).toEqual(["Open", "New PGN…"]);
    await expect(line()).not.toExist();
    await expect(list()).not.toExist();
    const open = button("Open");
    const create = button("New PGN…");
    expect(await open.getSize("height")).toBe(28);
    expect(await create.getSize("height")).toBe(28);
    const headingBottom =
      (await heading().getLocation("y")) + (await heading().getSize("height"));
    expect((await open.getLocation("y")) - headingBottom).toBeCloseTo(16);
    const openRight =
      (await open.getLocation("x")) + (await open.getSize("width"));
    expect((await create.getLocation("x")) - openRight).toBe(8);

    await open.click();
    await expect(button("PGN file…")).toBeDisplayed();
    await expect(button("Folder…")).toBeDisplayed();
    await open.click();
    expect(await pick("New", null)).toBeNull();
    await expect(list()).not.toExist();
  });

  it("lists a player's downloaded games in the order they were added", async () => {
    await browser.setWindowSize(1280, 800);
    await browser.$("nav").$("aria/Files").click();
    const study = `${work}/study.pgn`;
    const original = `[Event "Original"]
[Date "2025.12.14"]
[White "Elm"]
[Black "Ash"]
[Result "1/2-1/2"]

1. d4 {Keep my note} (1. Nf3) d5 1/2-1/2`;
    writeFileSync(study, original);
    await replyWithPlayerGames("games");
    const binary = fileURLToPath(
      new URL("../../../../target/debug/zobrist", import.meta.url),
    );
    const fixture = exportOrigin();
    const download = () => {
      const { status, stdout, stderr } = spawnSync(
        binary,
        [
          "pgn",
          "download-player",
          "RookExample",
          "--from",
          "2026-01-01",
          "--to",
          "2026-01-31",
          "study.pgn",
        ],
        {
          cwd: work,
          encoding: "utf8",
          env: {
            ZOBRIST_TEST_EXPORT_PORT: fixture.port,
            HTTPS_PROXY: fixture.origin,
          },
        },
      );
      expect({ status, stdout, stderr }).toEqual({
        status: 0,
        stdout: "games added: 2\n",
        stderr: "",
      });
    };
    download();
    const paths = await exportPaths();
    expect(paths).toHaveLength(1);
    const url = new URL(paths[0] ?? "", "http://127.0.0.1");
    expect(url.pathname).toBe("/api/games/user/RookExample");
    url.searchParams.sort();
    expect(url.search).toBe(
      "?pgnInJson=true&since=1767225600000&until=1769904000000",
    );
    expect(
      readFileSync(study).subarray(0, Buffer.byteLength(original)),
    ).toEqual(Buffer.from(original));

    await open(study);
    await expect(rows()).toBeElementsArrayOfSize(3);
    const contents = [
      ["1", "Elm", "Ash", "1/2-1/2", "2025.12.14", "Original"],
      ["2", "Birch", "RookExample", "0-1", "2026.01.31", "January 31"],
      ["3", "RookExample", "Aster", "1-0", "2026.01.01", "January 1"],
    ];
    expect(await cellTexts()).toEqual(contents);
    const picture = await browser.saveScreenshot(
      `${screenshots}/download-player-games.png`,
    );
    expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
      1280, 800,
    ]);

    download();
    await button("study.pgn").click();
    await expect(rows()).toBeElementsArrayOfSize(5);
    expect(await cellTexts()).toEqual([
      ...contents,
      ["4", "Birch", "RookExample", "0-1", "2026.01.31", "January 31"],
      ["5", "RookExample", "Aster", "1-0", "2026.01.01", "January 1"],
    ]);
  });

  it("remembers a tournament folder and a separate repertoire at their original locations", async () => {
    const folder = `${work}/Tournaments`;
    const tournament = `${folder}/Club.pgn`;
    const repertoire = `${work}/Repertoire.pgn`;
    mkdirSync(folder);
    copyFileSync(clean, tournament);
    copyFileSync(clean, repertoire);
    writeFileSync(`${folder}/Notes.txt`, "Match notes");
    await pick("Folder", folder);
    const tournaments = button("Tournaments");
    await expect(tournaments).toHaveAttribute("aria-current", "true");
    await expect(tournaments).toHaveAttribute("aria-expanded", "true");
    await expect(button("Club.pgn")).toBeDisplayed();
    await expect(button("Notes.txt")).not.toExist();
    expect(await pick("New", null)).toBe(folder);
    await expect(tournaments).toHaveAttribute("aria-current", "true");
    await expect(button("Club.pgn")).toBeDisplayed();
    await expect(list()).not.toExist();
    await open(repertoire);
    await expect(selection().$("h2")).toHaveText("Repertoire.pgn");
    await expect(pathLine(repertoire)).toBeDisplayed();
    await pick("File", null);
    await expect(button("Repertoire.pgn")).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(await cellTexts()).toEqual(cleanRows);

    await browser.refresh();
    await browser.$("nav").$("aria/Files").click();
    await expect(button("Repertoire.pgn")).toBeDisplayed();
    await tournaments.click();
    await expect(button("Club.pgn")).toBeDisplayed();
    await button("Club.pgn").click();
    await expect(pathLine(tournament)).toBeDisplayed();
    expect(await cellTexts()).toEqual(cleanRows);

    const picture = await browser.saveScreenshot(`${screenshots}/files.png`);
    // A PNG file holds its width and its height at these two places.
    expect({
      width: picture.readUInt32BE(16),
      height: picture.readUInt32BE(20),
    }).toEqual({ width: 1280, height: 800 });
  });

  it("creates test.pgn and updates its one game from the same tab", async () => {
    const folder = `${work}/Weekend`;
    mkdirSync(folder);
    const path = `${folder}/test.pgn`;
    const files = () => browser.$("nav").$("aria/Files").click();
    const tool = (name: string) => browser.$("main aside").$(`aria/${name}`);
    const form = () => browser.$("main aside form");
    await files();
    await expect(button("New PGN…")).toBeDisplayed();
    await pick("Folder", folder);
    await expect(button("Weekend")).toHaveAttribute("aria-current", "true");
    expect(await pick("New", `${folder}/test`)).toBe(folder);
    await expect(main().$$("button=test.pgn")).toBeElementsArrayOfSize(1);
    await expect(main().$(`button[title="${folder}"] + ul button`)).toHaveText(
      "test.pgn",
    );
    await expect(button("test.pgn")).toHaveAttribute("aria-current", "true");
    await expect(selection().$("h2")).toHaveText("test.pgn");
    await expect(pathLine(path)).toBeDisplayed();
    await expect(count("0 games")).toBeDisplayed();
    await expect(list()).toBeDisplayed();
    expect(await cellTexts()).toEqual([]);
    expect(readFileSync(path, "utf8")).toBe("");

    await browser.$("header").$("aria/New board").click();
    await tool("Show the game data").click();
    await form().$("aria/White").setValue("Alpha");
    await browser.keys("Enter");
    await tool("Save").click();
    await form().$("aria/Save").click();
    await expect(browser.$('header [role="tab"] span:empty')).not.toExist();
    await files();
    await expect(count("1 game")).toBeDisplayed();
    expect(await cellTexts()).toEqual([["1", "Alpha", "?", "*", "?", "?"]]);
    expect(readFileSync(path, "utf8")).toBe('[White "Alpha"]\n\n*\n');

    await rows()[0]?.click();
    await expect(tabs()).toBeElementsArrayOfSize(1);
    await expect(browser.$('header [role="tab"]')).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await tool("Show the game data").click();
    await form().$("aria/Black").setValue("Beta");
    await browser.keys(["Control", "s"]);
    await expect(browser.$('header [role="tab"]')).toHaveText("Alpha - Beta");
    await expect(browser.$('header [role="tab"] span:empty')).not.toExist();
    await files();
    await expect(count("1 game")).toBeDisplayed();
    await expect(list().$("td=Beta")).toBeDisplayed();
    expect(await cellTexts()).toEqual([["1", "Alpha", "Beta", "*", "?", "?"]]);
    expect(readFileSync(path, "utf8")).toBe(
      '[White "Alpha"]\n[Black "Beta"]\n\n*\n',
    );
    await browser.$("header").$("aria/Close Alpha - Beta").click();
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

    // The collection and the gap beside it take 264 px of the page.
    expect(await list().getSize("width")).toBe(1280 - 208 - 32 - 40 - 240 - 24);
    expect(await headCells().map((cell) => cell.getSize("width"))).toEqual([
      48,
      expect.closeTo(512 / 3, 0),
      expect.closeTo(512 / 3, 0),
      72,
      104,
      expect.closeTo(512 / 3, 0),
    ]);
    expect(await rows().map((row) => row.getSize("height"))).toEqual([28, 28]);
    const pathBottom =
      (await pathLine(clean).getLocation("y")) +
      (await pathLine(clean).getSize("height"));
    expect((await list().getLocation("y")) - pathBottom).toBeCloseTo(16);
  });

  it("lists the games of a file selected in a folder", async () => {
    await pick("Folder", fixtures);
    await expect(button("fixtures")).toHaveAttribute("aria-expanded", "true");
    await button("damaged.pgn").click();

    await expect(rows()).toBeElementsArrayOfSize(3);
    expect(await cellTexts()).toEqual([
      ["1", "Alpha", "Beta", "1-0", "?", "Example"],
      ["2", "Gamma", "Delta", "*", "?", "Example"],
      ["3", "Epsilon", "Zeta", "0-1", "?", "Example"],
    ]);
    await expect(line()).toHaveText("1 game has a problem.");
    expect(await lineColor()).toBe("#6b6b6b");
    const path = pathLine(`${fixtures}/damaged.pgn`);
    const rowBottom =
      (await path.getLocation("y")) + (await path.getSize("height"));
    const lineTop = await line().getLocation("y");
    expect(lineTop - rowBottom).toBeCloseTo(8);
    const lineBottom = lineTop + (await line().getSize("height"));
    expect((await list().getLocation("y")) - lineBottom).toBeCloseTo(16);
  });

  it("says why a file cannot be opened and shows no list", async () => {
    await browser.$("nav").$("aria/Files").click();
    await open(clean);
    await expect(rows()).toBeElementsArrayOfSize(2);
    expect(await cellTexts()).toEqual(cleanRows);
    await open(`${work}/missing.pgn`);

    await expect(line()).toHaveText(
      "This file cannot be opened. No such file or directory (os error 2)",
    );
    expect(await lineColor()).toBe("#b3261e");
    await expect(list()).not.toExist();
  });

  it("creates an empty file and never replaces a file", async () => {
    const created = `${work}/new.pgn`;
    await pick("New", created);

    await expect(selection().$("h2")).toHaveText("new.pgn");
    await expect(pathLine(created)).toBeDisplayed();
    await expect(count("0 games")).toBeDisplayed();
    await expect(list()).toBeDisplayed();
    await expect(rows()).toBeElementsArrayOfSize(0);
    expect(readFileSync(created, "utf8")).toBe("");

    writeFileSync(created, "1. e4 *\n");
    await pick("New", created);

    await expect(line()).toHaveText("This file exists already.");
    expect(await lineColor()).toBe("#b3261e");
    expect(readFileSync(created, "utf8")).toBe("1. e4 *\n");
  });

  it("says why a file cannot be created", async () => {
    await pick("New", `${work}/missing-folder/new.pgn`);

    await expect(line()).toHaveText(
      "This file cannot be created. No such file or directory (os error 2)",
    );
  });

  it("shows an answer that arrives after the page was left and shown again", async () => {
    const slow = `${work}/slow.pgn`;
    const letRustAnswer = await openHeldBack(slow);
    try {
      await browser.$("nav").$("aria/Home").click();
      await expect(heading()).toHaveText("Home");
      await browser.$("nav").$("aria/Files").click();
      await expect(heading()).toHaveText("Files");
    } finally {
      letRustAnswer();
    }

    await expect(pathLine(slow)).toBeDisplayed();
    await expect(rows()).toBeElementsArrayOfSize(1);
    expect(await cellTexts()).toEqual([["1", "Late", "?", "*", "?", "?"]]);
  });

  it("opens a board while Rust still reads a file", async () => {
    const letRustAnswer = await openHeldBack(`${work}/held.pgn`);
    try {
      await browser.$("header").$("aria/New board").click();
      await expect(tabs()).toBeElementsArrayOfSize(1);
    } finally {
      // A failed test must not leave Rust at the pipe, where it would hold up every later test.
      letRustAnswer();
    }

    await browser.$("nav").$("aria/Files").click();
    await expect(rows()).toBeElementsArrayOfSize(1);
    await browser.$("header").$("aria/Close New game").click();
    await expect(tabs()).toBeElementsArrayOfSize(0);
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
    await expect(pathLine(clean)).toBeDisplayed();
    expect(await cellTexts()).toEqual(cleanRows);
  });

  it("clears the collection error after the list is restored and Files is reopened", async () => {
    const files = () => browser.$("nav").$("aria/Files").click();
    const home = () => browser.$("nav").$("aria/Home").click();
    const { ZOBRIST_TEST_DATA: data } = process.env;
    const collection = `${data}/io.github.echo8134.zobrist/files.json`;
    await files();
    await open(clean);
    await expect(pathLine(clean)).toBeDisplayed();
    await home();
    const original = readFileSync(collection);
    try {
      writeFileSync(collection, "[[");
      await files();
      await expect(line()).toHaveText(
        `Could not read the file collection at ${collection}. Your existing PGN files are untouched. Choose Recover file list to restore the last good copy.`,
      );
    } finally {
      writeFileSync(collection, original);
    }

    await home();
    await files();
    await expect(heading()).toHaveText("Files");
    await expect(line()).not.toExist();
  });
});
