import {
  mkdirSync,
  mkdtempDisposableSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { withDesktopApp } from "./desktop-session";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const game = '[White "Ada"]\n[Black "Bo"]\n[Result "1-0"]\n\n1. e4 1-0\n';
const recent = (app: WebdriverIO.Browser) =>
  app.$('main section[aria-label="Recent files"]');
const selection = (app: WebdriverIO.Browser) =>
  app.$('main section[aria-label="Selection"]');
const choosePage = (app: WebdriverIO.Browser, page: "Home" | "Files") =>
  app.$("nav").$(`aria/${page}`).click();

async function expectRecent(app: WebdriverIO.Browser, paths: string[]) {
  const entries = recent(app).$$("button");
  await expect(entries).toBeElementsArrayOfSize(paths.length);
  for (const [index, path] of paths.entries()) {
    const entry = recent(app).$(`li:nth-child(${index + 1}) button`);
    const nameLabel = entry.$("span:first-child");
    const pathLabel = entry.$("span:last-child");
    await expect(nameLabel).toBeDisplayed();
    await expect(nameLabel).toHaveText(basename(path));
    await expect(pathLabel).toBeDisplayed();
    await expect(pathLabel).toHaveText(path);
  }
}

describe("recent PGN files on Home", () => {
  it("opens the selected Final.pgn and remembers it first after quitting", async function () {
    this.timeout(60_000);
    using work = mkdtempDisposableSync(join(tmpdir(), "zobrist-"));
    const club = join(work.path, "Club", "Final.pgn");
    const league = join(work.path, "League", "Final.pgn");
    mkdirSync(join(work.path, "Club"));
    mkdirSync(join(work.path, "League"));
    writeFileSync(club, game);
    writeFileSync(league, '[White "Lea"]\n\n1. d4 *\n');

    await withDesktopApp(join(work.path, "data"), async (app) => {
      await expect(app.$("main h1")).toHaveText("Home");
      await expect(recent(app).$("h2")).toHaveText("Recent files");
      await expect(recent(app).$("p")).toHaveText("No recent files yet.");
      await choosePage(app, "Files");
      for (const path of [club, league]) {
        await pick("File", path, app);
        await expect(selection(app).$(`p=${path}`)).toBeDisplayed();
      }
      await choosePage(app, "Home");
      await expectRecent(app, [league, club]);

      await recent(app).$(`button[title="${club}"]`).click();
      await expect(app.$("main h1")).toHaveText("Files");
      await expect(selection(app).$("h2")).toHaveText("Final.pgn");
      await expect(selection(app).$(`p=${club}`)).toBeDisplayed();
      await expect(selection(app).$$("tbody tr")).toBeElementsArrayOfSize(1);
      expect(
        await selection(app)
          .$$("tbody td")
          .map((cell) => cell.getText()),
      ).toEqual(["1", "Ada", "Bo", "1-0", "?", "?"]);
    });

    await withDesktopApp(join(work.path, "data"), async (app) => {
      await expect(app.$("main h1")).toHaveText("Home");
      await expect(app.$("aria/Lichess game link")).toBeDisplayed();
      await expectRecent(app, [club, league]);
      mkdirSync(screenshots, { recursive: true });
      const picture = await app.saveScreenshot(
        join(screenshots, "home-recent-files.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
      await choosePage(app, "Files");
      await app.$(`main button[title="${league}"]`).click();
      await expect(selection(app).$(`p=${league}`)).toBeDisplayed();
      await choosePage(app, "Home");
      await expectRecent(app, [league, club]);
    });
  });

  it("keeps the ten latest opens in order when a recent file is missing", async function () {
    this.timeout(60_000);
    using work = mkdtempDisposableSync(join(tmpdir(), "zobrist-"));
    const folder = join(work.path, "Games");
    mkdirSync(folder);
    const paths = Array.from({ length: 11 }, (_, index) =>
      join(folder, `${String(index + 1).padStart(2, "0")}.pgn`),
    );
    for (const path of paths) writeFileSync(path, game);
    await withDesktopApp(join(work.path, "data"), async (app) => {
      await choosePage(app, "Files");
      await pick("Folder", folder, app);
      for (const path of paths) {
        await app.$(`main button[title="${path}"]`).click();
        await expect(selection(app).$(`p=${path}`)).toBeDisplayed();
      }
      await pick("File", null, app);
      await choosePage(app, "Home");
      const latest = [
        "11",
        "10",
        "09",
        "08",
        "07",
        "06",
        "05",
        "04",
        "03",
        "02",
      ].map((number) => join(folder, `${number}.pgn`));
      await expectRecent(app, latest);
      const first = join(folder, "11.pgn");
      await recent(app).$(`button[title="${first}"]`).click();
      await expect(selection(app).$(`p=${first}`)).toBeDisplayed();
      await expect(app.$(`main button[title="${first}"]`)).toHaveAttribute(
        "aria-current",
        "true",
      );
      await choosePage(app, "Home");
      const missing = join(folder, "02.pgn");
      renameSync(missing, `${missing}.moved`);
      await recent(app).$(`button[title="${missing}"]`).click();
      await expect(selection(app).$('[role="status"]')).toHaveText(
        "This file cannot be opened. No such file or directory (os error 2)",
      );
      await choosePage(app, "Home");
      await expectRecent(app, latest);
    });
  });

  it("explains how to restore a damaged list and still opens a PGN", async () => {
    using work = mkdtempDisposableSync(join(tmpdir(), "zobrist-"));
    const directory = join(work.path, "data", "io.github.echo8134.zobrist");
    const list = join(directory, "recent-files.json");
    const path = join(work.path, "Club.pgn");
    mkdirSync(directory, { recursive: true });
    writeFileSync(list, "[[");
    writeFileSync(path, game);
    await withDesktopApp(join(work.path, "data"), async (app) => {
      await expect(recent(app).$("p")).toHaveText(
        `Could not read the recent files list at ${list}. Your existing PGN files are untouched. Restore the list from a backup or move it aside, then reopen Home.`,
      );
      await choosePage(app, "Files");
      await pick("File", path, app);
      await expect(selection(app).$(`p=${path}`)).toBeDisplayed();
      await expect(selection(app).$("td=Ada")).toBeDisplayed();
    });
    expect(readFileSync(list, "utf8")).toBe("[[");
  });
});
