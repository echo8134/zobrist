import {
  mkdirSync,
  mkdtempDisposableSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { withDesktopApp } from "./desktop-session";
import { pick } from "./file-picker";
import { expectRecent } from "./recent-files";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const game = '[White "Ada"]\n[Black "Bo"]\n[Result "1-0"]\n\n1. e4 1-0\n';

async function drop(app: WebdriverIO.Browser, paths: string[]) {
  await app.execute(
    'window.dispatchEvent(new CustomEvent("zobrist-file-drop", { detail: { paths: arguments[0] } }));',
    paths,
  );
  await expect(app.$("main h1")).toHaveText("Files");
}

describe("dropping files on the window", () => {
  it("opens a PGN from the board or Home and explains each refused drop", async function () {
    this.timeout(90_000);
    using work = mkdtempDisposableSync(join(tmpdir(), "zobrist-"));
    const club = join(work.path, "Club", "Final.PGN");
    const league = join(work.path, "League.pgn");
    const first = join(work.path, "A.pgn");
    const second = join(work.path, "B.pgn");
    const notes = join(work.path, "notes.txt");
    const archive = join(work.path, "Archive.pgn");
    const missing = join(work.path, "Missing.pgn");
    mkdirSync(join(work.path, "Club"));
    mkdirSync(archive);
    for (const path of [
      club,
      first,
      second,
      notes,
      join(archive, "game.pgn"),
      missing,
    ]) {
      writeFileSync(path, game);
    }
    writeFileSync(
      league,
      '[White "Lea"]\n[Black "Ivo"]\n[Result "0-1"]\n\n1. d4 0-1\n',
    );
    unlinkSync(missing);

    await withDesktopApp(join(work.path, "data"), async (app) => {
      const selection = () => app.$('main section[aria-label="Selection"]');
      const collection = () => app.$('main section[aria-label="Collection"]');
      const status = () => selection().$('[role="status"]');
      const tab = () => app.$('header [role="tab"]');
      const move = () => app.$('main aside ol [aria-current="true"]');
      const choose = (page: "Files" | "Home") =>
        app.$("nav").$(`aria/${page}`).click();

      async function expectSelection(
        path: string,
        white: string,
        black: string,
        result: string,
      ) {
        await expect(collection().$(`button[title="${path}"]`)).toHaveAttribute(
          "aria-current",
          "true",
        );
        await expect(selection().$("h2")).toHaveText(basename(path));
        await expect(selection().$(`p=${path}`)).toBeDisplayed();
        await expect(selection().$$("tbody tr")).toBeElementsArrayOfSize(1);
        expect(
          await selection()
            .$$("tbody td")
            .map((cell) => cell.getText()),
        ).toEqual(["1", white, black, result, "?", "?"]);
      }

      async function expectBothFiles() {
        await expect(collection().$$("button")).toBeElementsArrayOfSize(2);
        for (const path of [club, league]) {
          await expect(collection().$(`button[title="${path}"]`)).toHaveText(
            basename(path),
          );
        }
      }

      await app.$("header").$("aria/New board").click();
      const board = app.$("main cg-board");
      await expect(board).toBeDisplayed();
      const square = (await board.getSize("width")) / 8;
      for (const rank of [2, 4]) {
        await board.click({ x: square / 2, y: (4.5 - rank) * square });
      }
      await expect(move()).toHaveText("e4");
      await expect(tab()).toHaveAttribute("aria-label", "New game, not saved");
      const comment = () => app.$("main aside").$("aria/Comment");
      await comment().setValue("Take the centre");
      await expect(comment()).toBeFocused();
      await drop(app, [club]);
      await expectSelection(club, "Ada", "Bo", "1-0");
      mkdirSync(screenshots, { recursive: true });
      const picture = await app.saveScreenshot(
        join(screenshots, "drop-pgn.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
      await tab().click();
      await expect(move()).toHaveText("e4");
      await expect(
        app.$("main aside ol").$("span=Take the centre"),
      ).toBeDisplayed();
      await expect(comment()).toHaveValue("Take the centre");
      await expect(tab()).toHaveAttribute("aria-label", "New game, not saved");

      await choose("Files");
      await pick("File", league, app);
      await expectSelection(league, "Lea", "Ivo", "0-1");
      await choose("Home");
      await drop(app, [club]);
      await expectSelection(club, "Ada", "Bo", "1-0");
      await expectBothFiles();
      await choose("Home");
      await expectRecent(app, [club, league]);

      for (const { paths, message } of [
        { paths: [first, second], message: "Drop one PGN file at a time." },
        { paths: [first, notes], message: "Drop one PGN file at a time." },
        {
          paths: [archive],
          message: "To open a folder, use the Folder… option under Open.",
        },
        { paths: [notes], message: "Drop a file with the .pgn extension." },
      ]) {
        await drop(app, paths);
        await expect(status()).toHaveText(message);
        await expectSelection(club, "Ada", "Bo", "1-0");
        await expectBothFiles();
        await choose("Home");
        await expectRecent(app, [club, league]);
      }

      await drop(app, [missing]);
      await expect(status()).toHaveText(
        "This file cannot be opened. No such file or directory (os error 2)",
      );
      await expectBothFiles();
      await choose("Home");
      await expectRecent(app, [club, league]);
      await drop(app, [league]);
      await expectSelection(league, "Lea", "Ivo", "0-1");
      await expect(status()).not.toExist();
      await choose("Home");
      await expectRecent(app, [league, club]);
    });
  });

  it("keeps the player name being edited when a PGN is dropped", async () => {
    using work = mkdtempDisposableSync(join(tmpdir(), "zobrist-"));
    const club = join(work.path, "Club.pgn");
    writeFileSync(club, game);
    await withDesktopApp(join(work.path, "data"), async (app) => {
      await app.$("header").$("aria/New board").click();
      await app.$("main aside").$("aria/Show the game data").click();
      const form = () => app.$("main aside form");
      await form().$("aria/Black").setValue("Bo");
      await form().$("aria/White").setValue("Ada");
      await expect(form().$("aria/White")).toBeFocused();
      await drop(app, [club]);
      await app.$('header [role="tab"]').click();
      await expect(app.$('header [role="tab"]')).toHaveText("Ada - Bo");
      await expect(form().$("aria/White")).toHaveValue("Ada");
    });
  });

  it("keeps opening the selected file when a drop is refused", async () => {
    using work = mkdtempDisposableSync(join(tmpdir(), "zobrist-"));
    const club = join(work.path, "Club.pgn");
    const league = join(work.path, "League.pgn");
    writeFileSync(club, game);
    writeFileSync(league, "");
    await withDesktopApp(join(work.path, "data"), async (app) => {
      for (const path of [club, league]) {
        await drop(app, [path]);
        await expect(app.$('section[aria-label="Selection"] h2')).toHaveText(
          basename(path),
        );
      }
      const clubEntry = await app
        .$('section[aria-label="Collection"]')
        .$("button=Club.pgn");
      // Deliver both actions in one turn, before the window can receive the read's answer.
      await app.execute(
        `
        arguments[0].click();
        window.dispatchEvent(new CustomEvent("zobrist-file-drop", {detail: {paths: arguments[1]}}));
      `,
        clubEntry,
        [club, league],
      );
      const selection = app.$('section[aria-label="Selection"]');
      await expect(selection.$('[role="status"]')).toHaveText(
        "Drop one PGN file at a time.",
      );
      await expect(selection.$("h2")).toHaveText("Club.pgn");
      await expect(selection.$("td=Ada")).toBeDisplayed();
      await app.$("nav").$("aria/Home").click();
      await expectRecent(app, [club, league]);
    });
  });
});
