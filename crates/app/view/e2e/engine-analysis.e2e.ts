import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";
import { openEngines } from "./engine-session";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const panel = () => browser.$('section[aria-label="Engine analysis"]');
const control = (name: string) => panel().$(`aria/${name}`);
const named = (name: string) => browser.$("main").$(`aria/${name}`);
const rows = () => panel().$$("ol li");
const matePosition = "8/8/8/8/8/5kq1/8/7K b - - 0 1";

async function expectLines(expected: [string, string][]) {
  await expect(rows()).toBeElementsArrayOfSize(expected.length);
  for (const [index, [score, moves]] of expected.entries()) {
    const row = panel().$(`ol li:nth-child(${index + 1})`);
    await expect(row.$("strong")).toHaveText(score);
    await expect(row.$("span")).toHaveText(moves);
  }
}

async function clickSquare(square: string) {
  const board = browser.$("main cg-board");
  const side = (await board.getSize("width")) / 8;
  await board.click({
    x: ("abcdefgh".indexOf(square.charAt(0)) - 3.5) * side,
    y: (4.5 - Number(square.charAt(1))) * side,
  });
}

async function expectExit(executable: string) {
  const pid = Number(readFileSync(`${executable}.pid`, "utf8"));
  await browser.waitUntil(
    () => {
      try {
        process.kill(pid, 0);
        return false;
      } catch (error) {
        if (
          error instanceof Error &&
          "code" in error &&
          error.code === "ESRCH"
        ) {
          return true;
        }
        throw error;
      }
    },
    { timeoutMsg: `The engine process ${pid} still exists` },
  );
}

describe("the default engine on the board", () => {
  it("follows the position until stopped and exits when its board closes", async function () {
    this.timeout(60_000);
    mkdirSync(screenshots, { recursive: true });
    const work = mkdtempSync(join(screenshots, "analysis-"));
    const two = join(work, "two-lines");
    const one = join(work, "single-line");
    try {
      for (const executable of [two, one]) {
        copyFileSync(
          new URL("./analysis-engine.sh", import.meta.url),
          executable,
        );
        chmodSync(executable, 0o700);
      }
      await browser.$("header").$("aria/New board").click();
      await control("Start engine").click();
      await expect(panel().$('[role="alert"]')).toHaveText(
        "Choose a default engine on the Engines page.",
      );
      await browser.$("nav").$("aria/Engines").click();
      await pick("Engine", two);
      await expect(browser.$("main li").$("span=Default")).toBeDisplayed();
      await browser.$("header [role=tab]").click();
      await expect(control("Start engine")).toBeEnabled();
      expect(existsSync(`${two}.pid`)).toBe(false);
      await control("Start engine").click();
      const opening: [string, string][] = [
        ["+0.34", "1. e4 e5"],
        ["+0.12", "1. d4 d5"],
      ];
      await expectLines(opening);
      await expect(named("Save")).not.toBeEnabled();
      await expect(
        browser.$$("main aside ol li span > button"),
      ).toBeElementsArrayOfSize(0);
      const picture = await browser.saveScreenshot(
        join(screenshots, "engine-analysis.png"),
      );
      expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
        1280, 800,
      ]);

      writeFileSync(`${two}.hold`, "");
      await clickSquare("e2");
      await clickSquare("e4");
      await expect(named("e4")).toHaveAttribute("aria-current", "true");
      await browser.waitUntil(() =>
        readFileSync(`${two}.commands`, "utf8").includes(
          "position startpos moves e2e4\n",
        ),
      );
      await expectLines([]);
      rmSync(`${two}.hold`);
      await expectLines([["−0.27", "1... c5 2. Nf3"]]);
      await browser.keys("ArrowLeft");
      await expectLines(opening);
      await browser.keys("ArrowRight");
      await expectLines([["−0.27", "1... c5 2. Nf3"]]);
      await control("Stop engine").click();
      await expect(control("Start engine")).toBeEnabled();
      await expectLines([]);
      await expectExit(two);
      await expect(named("e4")).toHaveAttribute("aria-current", "true");
      await expect(named("Save")).toBeEnabled();
      await clickSquare("e7");
      await clickSquare("e5");
      await expect(named("e5")).toHaveAttribute("aria-current", "true");
      await expect(control("Start engine")).toBeEnabled();
      await expectLines([]);
      expect(
        readFileSync(`${two}.commands`, "utf8")
          .split("\n")
          .filter((line) => line === "uci"),
      ).toHaveLength(1);

      await browser.$("nav").$("aria/Engines").click();
      await pick("Engine", one);
      await browser.$("main").$("button=Make default").click();
      await browser.waitUntil(
        async () =>
          (await browser.$("main li strong").getText()) === "single-line",
      );
      await browser.$("header").$("aria/New board").click();
      await control("Start engine").click();
      await expectLines([["+0.34", "1. e4 e5"]]);
      await named("Set up position").click();
      await named("FEN").setValue(matePosition);
      await expect(named("Black")).toBeSelected();
      await named("Use position").click();
      await expectLines([["Black mates in 1", "1... Qg2#"]]);
      await browser.$("header [role=tab][aria-selected=true] + button").click();
      await browser.$("header").$("button=Close").click();
      await expectExit(one);
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });

  const departures: [
    string,
    (session: WebdriverIO.Browser) => Promise<unknown>,
  ][] = [
    [
      "opens another page",
      (session) => session.$("nav").$("aria/Home").click(),
    ],
    [
      "switches board tabs",
      (session) => session.$("header [role=tab][aria-selected=false]").click(),
    ],
    [
      "quits the app",
      (session) => {
        // Session cleanup cannot reconnect after the app's embedded driver exits.
        session.options.connectionRetryCount = 0;
        return expect(session.closeWindow()).rejects.toThrow(
          "All window handles were removed",
        );
      },
    ],
  ];
  for (const [description, leave] of departures) {
    it(`reaps the running engine when the player ${description}`, async () => {
      const work = mkdtempSync(join(tmpdir(), "zobrist-engine-exit-"));
      const program = join(work, "single-line");
      try {
        copyFileSync(new URL("./analysis-engine.sh", import.meta.url), program);
        chmodSync(program, 0o700);
        await openEngines(work, async (session) => {
          await pick("Engine", program, session);
          await expect(session.$("main li").$("span=Default")).toBeDisplayed();
          const newBoard = session.$("header").$("aria/New board");
          await newBoard.click();
          await newBoard.click();
          await expect(session.$$("header [role=tab]")).toBeElementsArrayOfSize(
            2,
          );
          const analysis = session.$('section[aria-label="Engine analysis"]');
          await analysis.$("aria/Start engine").click();
          await expect(analysis.$("li span")).toHaveText("1. e4 e5");

          await leave(session);
          await expectExit(program);
          expect(readFileSync(`${program}.commands`, "utf8")).toMatch(
            /\nquit\n$/,
          );
        });
      } finally {
        rmSync(work, { recursive: true, force: true });
      }
    });
  }
});
