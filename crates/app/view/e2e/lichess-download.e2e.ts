import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { visitPage } from "./desktop-session";
import { answerNextPicker, pick } from "./file-picker";
import { exportOrigin, exportPaths, replyWithPlayerGames } from "./game-export";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const original =
  '[Event "Original"]\n[Result "*"]\n\n1. e4 {Keep my note} (1. Nf3) e5 *';
const january =
  "/api/games/user/RookExample?since=1767225600000&until=1769904000000&pgnInJson=true";

describe("downloading public games from User", () => {
  it("appends only the chosen dates, keeps earlier states, and allows a retry", async () => {
    mkdirSync(screenshots, { recursive: true });
    const work = mkdtempSync(join(screenshots, "player-export-"));
    const study = join(work, "study.pgn");
    writeFileSync(study, original);
    const events = () =>
      Array.from(
        readFileSync(study, "utf8").matchAll(/^\[Event "([^"]+)"\]/gm),
        ([, event]) => event,
      );
    const bytes = () => readFileSync(study);
    try {
      await replyWithPlayerGames("games");
      await visitPage(
        work,
        "User",
        async (app) => {
          await app.setWindowSize(1280, 800);
          await app.$("main summary").click();
          const form = app.$('form[aria-label="Public Lichess games"]');
          const submit = form.$("button=Download games");
          const status = form.$('[role="status"]');
          const date = (label: string) => form.$(`aria/${label}`);
          const choose = async (path: string | null) => {
            await answerNextPicker("PickPgnPath", path, app);
            await submit.click();
            await app.waitUntil(async () =>
              Boolean(await app.execute("return window.pickerRequest")),
            );
            expect(
              await app.execute("return window.pickerRequest.choice"),
            ).toBe("File");
          };
          await date("From").setValue("2026-01-01");
          await date("To").setValue("2026-01-31");
          await expect(submit).toBeDisabled();
          const username = app.$("aria/Lichess username");
          await username.setValue("RookExample");
          await app.$("button=Save").click();
          const summaryName = app.$("main summary span:last-of-type");
          await expect(summaryName).toHaveText("RookExample");
          await username.setValue("UnsavedExample");
          await expect(submit).toBeEnabled();
          expect(await exportPaths()).toEqual([]);
          await choose(null);
          await expect(submit).toBeEnabled();
          expect(await exportPaths()).toEqual([]);
          expect(bytes()).toEqual(Buffer.from(original));
          await date("From").setValue("2026-02-01");
          await expect(form.$('[role="alert"]')).toHaveText(
            "From must be on or before To.",
          );
          await expect(submit).toBeDisabled();
          expect(await exportPaths()).toEqual([]);
          await date("From").setValue("2026-01-01");
          await username.setValue("RookExample");
          await choose(study);
          await expect(status).toHaveText("Added 2 games to study.pgn.");
          expect(await exportPaths()).toEqual([january]);
          expect(bytes().subarray(0, Buffer.byteLength(original))).toEqual(
            Buffer.from(original),
          );
          expect(events()).toEqual(["Original", "January 31", "January 1"]);
          const first = bytes();
          const image = await app.saveScreenshot(
            join(screenshots, "user-lichess-download.png"),
          );
          expect([image.readUInt32BE(16), image.readUInt32BE(20)]).toEqual([
            1280, 800,
          ]);

          await replyWithPlayerGames("refused");
          await choose(study);
          await expect(status).toHaveText("Player not found.");
          expect(await exportPaths()).toEqual([january]);
          expect(bytes()).toEqual(first);
          await expect(summaryName).toHaveText("RookExample");
          await replyWithPlayerGames("empty");
          await choose(study);
          await expect(status).toHaveText("Added 0 games to study.pgn.");
          expect(await exportPaths()).toEqual([january]);
          expect(bytes()).toEqual(first);
          await replyWithPlayerGames("hold");
          await username.setValue("UnsavedExample");
          try {
            await answerNextPicker("PickPgnPath", study, app);
            // Deliver both clicks while the first picker answer is still pending.
            await app.execute(
              "arguments[0].click(); arguments[0].click();",
              await submit.getElement(),
            );
            await app.waitUntil(async () => (await exportPaths()).length > 0);
            await app.$("button=Save").click();
            await expect(summaryName).toHaveText("UnsavedExample");
          } finally {
            await replyWithPlayerGames("games");
          }
          await expect(status).toHaveText("RookExample: Added 2 games to study.pgn.");
          expect(await exportPaths()).toEqual([january]);
          expect(bytes().subarray(0, first.length)).toEqual(first);
          expect(events()).toEqual([
            "Original",
            "January 31",
            "January 1",
            "January 31",
            "January 1",
          ]);

          await app.$("nav").$("aria/Files").click();
          await pick("File", study, app);
          await expect(
            app.$('section[aria-label="Selection"]').$(`p=${study}`),
          ).toBeDisplayed();
          await app.$("main").$("button=Earlier states").click();
          const history = app.$('section[aria-label="Earlier states"]');
          await expect(history.$$("li")).toBeElementsArrayOfSize(2);
          await expect(history.$("aria/State 1")).toHaveText(
            expect.stringContaining(`${Buffer.byteLength(original)} bytes`),
          );
          await expect(history.$("aria/State 2")).toHaveText(
            expect.stringContaining(`${first.length} bytes`),
          );
        },
        { ZOBRIST_TEST_EXPORT_PORT: exportOrigin().port },
      );
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
});
