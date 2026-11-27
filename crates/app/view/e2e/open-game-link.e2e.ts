import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";
import { drawnBoard, startPositionWithWhiteAtBottom } from "./board";
import { exportRequests, replyWith } from "./game-export";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const link = "https://lichess.org/Ab12Cd34";
const pgn = `[White "Aster"]
[Black "Birch"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# 0-1
`;
const home = () => browser.$("nav").$("aria/Home");
const field = () => browser.$("main").$("aria/Lichess game link");
const open = () => browser.$("main").$("button=Open game link");
const tabs = () => browser.$$('header [role="tab"]');
const currentTab = () => browser.$('header [aria-selected="true"]');
const line = () => browser.$('main [role="status"]');

describe("opening a game link on Home", () => {
  it("opens a second board at the starting position and replays the game", async () => {
    await browser.setWindowSize(1280, 800);
    await replyWith(200, pgn);
    await browser.$("header").$("aria/New board").click();
    await expect(currentTab()).toHaveText("New game");
    await home().click();
    await field().setValue(`${link}/black#4`);
    expect(await exportRequests()).toBe(0);
    await open().click();

    await expect(currentTab()).toHaveText("Aster - Birch");
    expect(await tabs().map((tab) => tab.getText())).toEqual([
      "New game",
      "Aster - Birch",
    ]);
    await expect(browser.$("main cg-board piece")).toExist();
    expect(await drawnBoard()).toEqual(startPositionWithWhiteAtBottom);
    expect(
      await browser.$$("main aside ol button").map((move) => move.getText()),
    ).toEqual(["f3", "e5", "g4", "Qh4#"]);
    await browser.$("main aside ol").$("button=Qh4#").click();
    await expect(browser.$("main aside ol [aria-current]")).toHaveText("Qh4#");
    await browser.waitUntil(async () =>
      isDeepStrictEqual(await drawnBoard(), [
        "rnb.kbnr",
        "pppp.ppp",
        "........",
        "....p...",
        "......Pq",
        ".....P..",
        "PPPPP..P",
        "RNBQKBNR",
      ]),
    );
    expect(await exportRequests()).toBe(1);

    mkdirSync(screenshots, { recursive: true });
    const picture = await browser.saveScreenshot(
      `${screenshots}/open-game-link.png`,
    );
    expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
      1280, 800,
    ]);
  });

  it("keeps both tabs when a link or its export is refused", async () => {
    await home().click();
    for (const [url, status, body, message, requests] of [
      [
        "https://example.invalid/Ab12Cd34",
        200,
        pgn,
        "Enter a public Lichess game link.",
        0,
      ],
      [link, 404, "", "Game not found.", 1],
      [
        link,
        200,
        '[Result "*"]\n\n1. f3 *\n',
        "This game has no final result.",
        1,
      ],
      [link, 429, "", "Too many requests. Try again later.", 1],
    ] as const) {
      await replyWith(status, body);
      await field().setValue(url);
      await open().click();
      await expect(line()).toHaveText(message);
      expect(await tabs().map((tab) => tab.getText())).toEqual([
        "New game",
        "Aster - Birch",
      ]);
      expect(await exportRequests()).toBe(requests);
    }
    await open().click();
    await expect(open()).toBeEnabled();
    await expect(line()).toHaveText("Too many requests. Try again later.");
    expect(await exportRequests()).toBe(1);
  });
});
