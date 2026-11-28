import {
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { play, waitForSession } from "./board";
import { visitPage } from "./desktop-session";
import { answerNextPicker } from "./file-picker";

const root = fileURLToPath(new URL("../../../../", import.meta.url));
const clean = join(root, "crates/cli/tests/fixtures/clean.pgn");
const damaged = join(root, "crates/cli/tests/fixtures/damaged.pgn");

async function startImport(
  app: WebdriverIO.Browser,
  source: string | null,
  destination: string | null,
) {
  // The second dialog opens immediately after the first answer. Arm its reply first.
  if (source !== null) {
    await answerNextPicker("PickNewDatabasePath", destination, app);
  }
  await answerNextPicker("PickPgnPath", source, app);
  const button = app.$("button=Import PGN…");
  await button.click();
  const lastPicker = source === null ? "PickPgnPath" : "PickNewDatabasePath";
  await app.waitUntil(() =>
    app.execute<boolean, [string]>(
      "return window.pickerRequest?.type === arguments[0]",
      lastPicker,
    ),
  );
}

async function importPgn(
  app: WebdriverIO.Browser,
  source: string | null,
  destination: string | null,
) {
  await startImport(app, source, destination);
  await expect(app.$("button=Import PGN…")).toBeEnabled();
}

async function holdImport(app: WebdriverIO.Browser, destination: string) {
  // Keep the request until the player has left the page. Rust still performs the import.
  await app.execute(`
    const original = window.fetch;
    const endpoint = window.__TAURI_INTERNALS__.convertFileSrc("answer", "ipc");
    window.fetch = function(input, options) {
      if (String(input) === endpoint && typeof options?.body === "string" &&
          JSON.parse(options.body).request.type === "ImportDatabase") {
        window.fetch = original;
        const {promise, resolve} = Promise.withResolvers();
        window.releaseImport = async () => {
          const response = await original.call(window, input, options);
          resolve(response);
          await response.clone().text();
        };
        return promise;
      }
      return original.call(window, input, options);
    };
  `);
  await startImport(app, damaged, destination);
  await app.waitUntil(() =>
    app.execute<boolean, []>(
      "return typeof window.releaseImport === 'function'",
    ),
  );
  return () => app.execute("return window.releaseImport()");
}

describe("import a PGN into a local collection", () => {
  let directory: string;
  let collections: string;

  beforeEach(() => {
    directory = mkdtempSync(join(tmpdir(), "zobrist-import-"));
    collections = join(directory, "collections");
    mkdirSync(collections);
  });

  afterEach(() => rmSync(directory, { recursive: true, force: true }));

  function onDatabases(check: (app: WebdriverIO.Browser) => Promise<void>) {
    return visitPage(directory, "Databases", async (app) => {
      await expect(app.$("button=Import PGN…")).toBeEnabled();
      await check(app);
    });
  }

  it("selects the imported collection and reopens it after quitting", async () => {
    const study = join(collections, "Study.zobrist");
    await onDatabases(async (app) => {
      await importPgn(app, clean, study);
      await expect(
        app.$('[aria-label="This device"]').$("button=Study.zobrist"),
      ).toHaveAttribute("aria-current", "true");
      await expect(app.$("span=2 games")).toBeDisplayed();
      await expect(app.$("p=0 problems")).toBeDisplayed();
      const image = await app.saveScreenshot(
        join(root, "target/e2e/databases-import.png"),
      );
      expect([image.readUInt32BE(16), image.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
    });
    await onDatabases(async (app) => {
      await app.$("button=Study.zobrist").click();
      await expect(app.$("span=2 games")).toBeDisplayed();
    });
  });

  it("shows the first problem and opens the recovered games", async () => {
    await onDatabases(async (app) => {
      await importPgn(app, damaged, join(collections, "Recovered.zobrist"));
      await expect(app.$("span=3 games")).toBeDisplayed();
      await expect(app.$("p=1 problem")).toBeDisplayed();
      await expect(app.$("main [role=status] p:last-child")).toHaveText(
        'game 2 (Gamma - Delta): illegal move "Ke3" at White\'s move 2, 2 moves kept',
      );
      await app.$("button=Recovered.zobrist").click();
      await expect(app.$("span=3 games")).toBeDisplayed();
    });
  });

  it("shows the completed import after the player returns from a board", async () => {
    await onDatabases(async (app) => {
      const release = await holdImport(
        app,
        join(collections, "Recovered.zobrist"),
      );
      try {
        await app.$("header").$("aria/New board").click();
        await expect(app.$('header [role="tab"]')).toHaveAttribute(
          "aria-selected",
          "true",
        );
        await play(app, "e2", "e4", "e4");
        const session = join(
          directory,
          "data",
          "io.github.echo8134.zobrist",
          "boards.json",
        );
        await waitForSession(app, session, {
          boards: [{ game: "1. e4 *\n", place: [0], origin: null }],
          selected: 0,
        });
      } finally {
        await release();
      }
      await app.$("nav").$("aria/Databases").click();
      await expect(app.$("button=Recovered.zobrist")).toHaveAttribute(
        "aria-current",
        "true",
      );
      await expect(app.$("span=3 games")).toBeDisplayed();
      await expect(app.$("p=1 problem")).toBeDisplayed();
      await expect(app.$("main [role=status] p:last-child")).toHaveText(
        'game 2 (Gamma - Delta): illegal move "Ke3" at White\'s move 2, 2 moves kept',
      );
    });
  });

  it("shows the existing collection and import progress when returning before completion", async () => {
    await onDatabases(async (app) => {
      await importPgn(app, clean, join(collections, "Study.zobrist"));
      const release = await holdImport(
        app,
        join(collections, "Recovered.zobrist"),
      );
      await app.$("nav").$("aria/Home").click();
      await app.$("nav").$("aria/Databases").click();
      try {
        await expect(app.$("button=Importing PGN…")).toBeDisplayed();
        await expect(app.$("button=Study.zobrist")).toBeDisplayed();
      } finally {
        await release();
      }
      await expect(app.$("p=1 problem")).toBeDisplayed();
      await app.$("button=Study.zobrist").click();
      await expect(app.$("span=2 games")).toBeDisplayed();
      await expect(app.$("p=1 problem")).not.toExist();
    });
  });

  it("keeps the saved path when registration fails while another page is open", async () => {
    await onDatabases(async (app) => {
      const study = join(collections, "Study.zobrist");
      const listFolder = join(directory, "data", "io.github.echo8134.zobrist");
      const list = join(listFolder, "databases.json");
      const release = await holdImport(app, study);
      await app.$("nav").$("aria/Home").click();
      mkdirSync(listFolder, { recursive: true });
      writeFileSync(list, "[");
      await release();
      await app.$("nav").$("aria/Databases").click();
      await expect(app.$("main p[role=status]")).toHaveText(
        `The collection was saved at ${study}. Could not read the database collection at ${list}. Your existing database files are untouched. Restore the list from a backup or move it aside, then reopen Databases. Use “Add collection…” to register the saved file.`,
      );
    });
  });

  it("creates no file or list entry when either dialog is cancelled", async () => {
    await onDatabases(async (app) => {
      for (const source of [null, clean]) {
        await importPgn(app, source, null);
        await expect(app.$("p=No collections yet.")).toBeDisplayed();
        expect(readdirSync(collections)).toEqual([]);
      }
    });
  });

  it("preserves an existing file and allows a fresh destination on retry", async () => {
    await onDatabases(async (app) => {
      const study = join(collections, "Study.zobrist");
      await importPgn(app, clean, study);
      await expect(app.$("span=2 games")).toBeDisplayed();
      const original = readFileSync(study);
      await importPgn(app, damaged, study);
      await expect(app.$("main p[role=status]")).toHaveText(
        `A file already exists at ${study}. Choose a new file.`,
      );
      expect(readFileSync(study)).toEqual(original);
      await app.$("button=Study.zobrist").click();
      await expect(app.$("span=2 games")).toBeDisplayed();
      await importPgn(app, damaged, join(collections, "Retry.zobrist"));
      await expect(app.$("button=Retry.zobrist")).toHaveAttribute(
        "aria-current",
        "true",
      );
      await expect(app.$("span=3 games")).toBeDisplayed();
      await expect(app.$("button=Study.zobrist")).toBeDisplayed();
      await expect(
        app.$$('[aria-label="This device"] li'),
      ).toBeElementsArrayOfSize(2);
    });
  });
});
