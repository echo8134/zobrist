import {
  chmodSync,
  existsSync,
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
import { visitPage } from "./desktop-session";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const collection = (app: WebdriverIO.Browser) =>
  app.$('section[aria-label="Collection"]');
const status = (app: WebdriverIO.Browser) =>
  collection(app).$('[role="status"]');
const recover = (app: WebdriverIO.Browser) =>
  collection(app).$("aria/Recover file list");
const contents = (path: string) => readFileSync(path, "utf8");

async function checkEntries(app: WebdriverIO.Browser) {
  const rows = collection(app).$$("ul > li > button");
  await expect(rows).toBeElementsArrayOfSize(2);
  expect(await rows.map((button) => button.getText())).toEqual([
    "Club",
    "Study.pgn",
  ]);
}

async function reopen(app: WebdriverIO.Browser) {
  await app.$("nav").$("aria/Home").click();
  await app.$("nav").$("aria/Files").click();
}

describe("recovery of the Files collection", () => {
  let work: string;
  let data: string;
  let club: string;
  let study: string;
  let marker: string;
  let live: string;
  let backup: string;
  let entries: string[][];
  let recoveryMessage: string;
  let manualMessage: string;

  beforeEach(() => {
    work = mkdtempSync(join(tmpdir(), "zobrist-recover-"));
    data = join(work, "data", "io.github.echo8134.zobrist");
    club = join(work, "Club");
    study = join(work, "Study.pgn");
    const fixture = join(data, "pgn-history", "fixture");
    marker = join(fixture, "marker");
    mkdirSync(fixture, { recursive: true });
    writeFileSync(marker, "history");
    mkdirSync(club);
    writeFileSync(study, "1. e4 *\n");
    live = join(data, "files.json");
    backup = join(data, "files.last-good.json");
    entries = [
      ["Folder", club],
      ["File", study],
    ];
    const unreadable = `Could not read the file collection at ${live}. Your existing PGN files are untouched.`;
    recoveryMessage = `${unreadable} Choose Recover file list to restore the last good copy.`;
    manualMessage = `${unreadable} Restore the list from a backup or move it aside, then reopen Files.`;
  });
  afterEach(() => {
    rmSync(work, { recursive: true, force: true });
  });

  it("keeps both chosen entries across restarts and preserves the damaged list on recovery", async () => {
    await visitPage(work, "Files", async (app) => {
      await pick("Folder", club, app);
      await expect(collection(app).$("button=Club")).toHaveAttribute(
        "aria-current",
        "true",
      );
      await pick("File", study, app);
      await checkEntries(app);
    });
    const good = contents(live);
    expect(JSON.parse(good)).toEqual(entries);
    expect(contents(backup)).toBe(good);
    await visitPage(work, "Files", checkEntries);

    writeFileSync(live, "[[");
    const first = join(data, "files.damaged-1.json");
    const second = join(data, "files.damaged-2.json");
    writeFileSync(first, "keep");
    await visitPage(work, "Files", async (app) => {
      await expect(status(app)).toHaveText(recoveryMessage);
      await expect(recover(app)).toBeDisplayed();
      mkdirSync(screenshots, { recursive: true });
      const picture = await app.saveScreenshot(
        join(screenshots, "files-recover-list.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);
      await reopen(app);
      await expect(status(app)).toHaveText(recoveryMessage);
      expect(contents(live)).toBe("[[");
      expect(existsSync(second)).toBe(false);
      await recover(app).click();
      await checkEntries(app);
      await expect(recover(app)).not.toExist();
    });
    await visitPage(work, "Files", checkEntries);
    expect(contents(live)).toBe(good);
    expect(contents(backup)).toBe(good);
    expect(contents(first)).toBe("keep");
    expect(contents(second)).toBe("[[");
    expect(contents(study)).toBe("1. e4 *\n");
    expect(contents(marker)).toBe("history");
  });

  it("seeds the last good copy from a valid collection from an earlier version", async () => {
    const original = `${JSON.stringify(entries, null, 2)}\n`;
    writeFileSync(live, original);
    expect(existsSync(backup)).toBe(false);
    await visitPage(work, "Files", checkEntries);
    expect(contents(live)).toBe(original);
    expect(contents(backup)).toBe(original);
  });

  it("gives manual instructions when the backup is missing or damaged", async () => {
    writeFileSync(live, "[[");
    await visitPage(work, "Files", async (app) => {
      await expect(status(app)).toHaveText(manualMessage);
      await expect(recover(app)).not.toExist();
      expect(existsSync(backup)).toBe(false);
      expect(contents(live)).toBe("[[");
      writeFileSync(backup, "!");
      await reopen(app);
      await expect(status(app)).toHaveText(
        `${manualMessage} The last good copy at ${backup} could not be read.`,
      );
      await expect(recover(app)).not.toExist();
    });
    expect(contents(live)).toBe("[[");
    expect(contents(backup)).toBe("!");
    expect(contents(study)).toBe("1. e4 *\n");
    expect(contents(marker)).toBe("history");
  });

  it("leaves both lists in place when recovery fails and lets the player retry", async () => {
    const good = JSON.stringify(entries);
    writeFileSync(backup, good);
    writeFileSync(live, "[[");
    await visitPage(work, "Files", async (app) => {
      await expect(status(app)).toHaveText(recoveryMessage);
      chmodSync(data, 0o555);
      try {
        await recover(app).click();
        await expect(status(app)).toHaveText(
          `Could not preserve the damaged list at ${live}. Your existing PGN files are untouched. Allow writing to the app data folder and try again.`,
        );
        expect(contents(live)).toBe("[[");
        expect(contents(backup)).toBe(good);
        expect(
          readdirSync(data).filter((name) => name.startsWith("files.damaged-")),
        ).toEqual([]);
      } finally {
        chmodSync(data, 0o755);
      }
      await recover(app).click();
      await checkEntries(app);
    });
    expect(contents(live)).toBe(good);
    expect(contents(backup)).toBe(good);
    expect(contents(join(data, "files.damaged-1.json"))).toBe("[[");
    expect(contents(study)).toBe("1. e4 *\n");
    expect(contents(marker)).toBe("history");
  });
});
