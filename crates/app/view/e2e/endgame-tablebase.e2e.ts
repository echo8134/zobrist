import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { browser, expect } from "@wdio/globals";
import { exportPaths, releaseTablebase } from "./game-export";

const position = "4k3/3Q4/8/8/8/8/8/K7 b - - 17 42";
const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const tabs = () => browser.$$("header [role=tab]");
const tab = (number: number) =>
  browser.$(`header [role=tablist] > :nth-child(${number}) [role=tab]`);
const newBoard = () => browser.$("header").$("aria/New board");
const panel = () =>
  browser.$('section[aria-label="Lichess endgame tablebase"]');
const ask = () => panel().$("button=Ask Lichess");
const refusal = "Lichess tablebase is unavailable. Try again.";
let firstRequest: number;
const requests = async () => (await exportPaths()).slice(firstRequest);

async function openPosition(fen: string) {
  const count = await tabs().length;
  await browser.$("main").$("button=Paste PGN").click();
  await browser
    .$("main")
    .$("aria/PGN text")
    .setValue(`[SetUp "1"]\n[FEN "${fen}"]\n\n*`);
  await browser.$("main").$("button=Open as new game").click();
  await expect(tabs()).toBeElementsArrayOfSize(count + 1);
  await expect(panel()).toBeDisplayed();
}

async function playKingToF8() {
  const board = browser.$("main cg-board");
  const square = (await board.getSize("width")) / 8;
  for (const file of ["e", "f"]) {
    await board.click({
      x: ("abcdefgh".indexOf(file) - 3.5) * square,
      y: -3.5 * square,
    });
  }
  await expect(browser.$("main aside ol [aria-current]")).toHaveText("Kf8");
}

async function pendingCallbacks(): Promise<number[]> {
  const callbacks: unknown = await browser.execute(
    "return [...window.__TAURI_INTERNALS__.callbacks.keys()]",
  );
  if (!Array.isArray(callbacks)) throw new Error("Missing desktop callbacks.");
  return callbacks;
}

async function clickAndHold() {
  const before = await requests();
  const previous = await pendingCallbacks();
  await ask().click();
  await browser.waitUntil(
    async () => (await requests()).length === before.length + 1,
  );
  await expect(ask()).toBeDisabled();
  const path = (await requests()).at(-1);
  const url = new URL(path ?? "", "http://127.0.0.1");
  expect(url.pathname).toBe("/standard");
  expect([...url.searchParams]).toEqual([["fen", position]]);

  // Wait for the held desktop reply to reach the webview before checking for a stale result.
  const added = (await pendingCallbacks()).filter(
    (id) => !previous.includes(id),
  );
  expect(added.length).toBeGreaterThan(0);
  return async (status: 200 | 503) => {
    await releaseTablebase(status);
    await browser.waitUntil(async () =>
      (await pendingCallbacks()).every((id) => !added.includes(id)),
    );
  };
}

async function expectResult() {
  await expect(panel()).toHaveText(
    expect.stringContaining("Black to move: Draw"),
  );
  expect(
    await panel()
      .$$("th")
      .map((header) => header.getText()),
  ).toEqual(["Move", "Outcome for Black"]);
  const cells = await panel()
    .$$("tbody td")
    .map((cell) => cell.getText());
  expect(cells).toEqual(["Kxd7", "Draw", "Kf8", "Loss"]);
}

async function expectIdle() {
  await expect(ask()).toBeEnabled();
  const content = await panel().getText();
  expect(content).not.toContain("to move:");
  expect(content).not.toContain(refusal);
  expect(await panel().$$("tbody tr").length).toBe(0);
}

describe("asking the Lichess endgame tablebase", () => {
  beforeEach(async () => {
    firstRequest = (await exportPaths()).length;
    const count = await tabs().length;
    await newBoard().click();
    await expect(tabs()).toBeElementsArrayOfSize(count + 1);
  });

  it("names the recipient before the click and shows the outcome for Black", async () => {
    await browser.setWindowSize(1280, 800);
    await openPosition(position);
    await expect(panel()).toHaveText(
      expect.stringContaining(
        "Sends this position to the Lichess endgame tablebase.",
      ),
    );
    expect(await requests()).toEqual([]);
    const release = await clickAndHold();
    await release(200);
    await expectResult();
    await expect(browser.$("main cg-board")).toBeDisplayed();
    await expect(browser.$("main aside")).toBeDisplayed();
    mkdirSync(screenshots, { recursive: true });
    const image = await browser.saveScreenshot(
      `${screenshots}/lichess-tablebase.png`,
    );
    expect([image.readUInt32BE(16), image.readUInt32BE(20)]).toEqual([
      1280, 800,
    ]);
    await playKingToF8();
    await expectIdle();
    expect(await requests()).toHaveLength(1);
  });

  it("declines a board with more than seven pieces locally", async () => {
    await expect(ask()).toBeDisabled();
    await expect(panel()).toHaveText(
      expect.stringContaining(
        "This lookup supports at most 7 pieces (32 on the board).",
      ),
    );
    await openPosition("4k3/3Q4/8/8/8/8/PPPPP3/K7 b - - 0 1");
    await expect(ask()).toBeDisabled();
    await expect(panel()).toHaveText(
      expect.stringContaining(
        "This lookup supports at most 7 pieces (8 on the board).",
      ),
    );
    await openPosition("4k3/3Q4/8/8/8/8/PPPP4/K7 b - - 0 1");
    await expect(ask()).toBeEnabled();
    expect(await requests()).toEqual([]);
  });

  it("shows the refusal and retries only on the next click", async () => {
    await openPosition(position);
    const refuse = await clickAndHold();
    await refuse(503);
    await expect(panel()).toHaveText(expect.stringContaining(refusal));
    expect(await panel().$$("tbody tr").length).toBe(0);
    expect(await requests()).toHaveLength(1);
    const retry = await clickAndHold();
    await retry(200);
    await expectResult();
    expect(await requests()).toHaveLength(2);
  });

  it("ignores a reply after moving away and returning to the same position", async () => {
    await openPosition(position);
    const release = await clickAndHold();
    await playKingToF8();
    await browser.keys("ArrowLeft");
    await expect(browser.$("main aside ol [aria-current]")).not.toExist();
    await release(200);
    await expectIdle();
    expect(await requests()).toHaveLength(1);
  });

  it("ignores a held refusal on both tabs even when their positions match", async () => {
    await openPosition(position);
    const first = await tabs().length;
    await openPosition(position);
    const second = first + 1;
    const release = await clickAndHold();
    await tab(first).click();
    await expect(tab(first)).toHaveAttribute("aria-selected", "true");
    await release(503);
    await expectIdle();
    await tab(second).click();
    await expect(tab(second)).toHaveAttribute("aria-selected", "true");
    await expectIdle();
    expect(await requests()).toHaveLength(1);
  });
});
