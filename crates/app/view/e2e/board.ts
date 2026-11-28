import { existsSync, readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
import { browser, expect } from "@wdio/globals";

export const drawnBoard = (app: WebdriverIO.Browser = browser) =>
  app.execute(`
    const board = document.querySelector("main cg-board").getBoundingClientRect();
    const letters = { pawn: "p", knight: "n", bishop: "b", rook: "r", queen: "q", king: "k" };
    const rows = Array.from({ length: 8 }, () => Array(8).fill("."));
    for (const piece of document.querySelectorAll("main cg-board piece")) {
      const { x, y } = piece.getBoundingClientRect();
      const letter = letters[piece.classList[1]];
      const row = Math.round((y - board.y) / (board.height / 8));
      const column = Math.round((x - board.x) / (board.width / 8));
      rows[row][column] = piece.classList[0] === "white" ? letter.toUpperCase() : letter;
    }
    return rows.map((row) => row.join(""));
  `);
export const startPositionWithWhiteAtBottom = [
  "rnbqkbnr",
  "pppppppp",
  "........",
  "........",
  "........",
  "........",
  "PPPPPPPP",
  "RNBQKBNR",
];

export const expectMove = (app: WebdriverIO.Browser, move: string) =>
  expect(app.$("main aside ol").$(`button=${move}`)).toHaveAttribute(
    "aria-current",
    "true",
  );

export async function play(
  app: WebdriverIO.Browser,
  from: string,
  to: string,
  move: string,
) {
  const board = app.$("main cg-board");
  const side = (await board.getSize("width")) / 8;
  for (const square of [from, to]) {
    await board.click({
      x: ("abcdefgh".indexOf(square.charAt(0)) - 3.5) * side,
      y: (4.5 - Number(square.charAt(1))) * side,
    });
  }
  await expectMove(app, move);
}

export async function waitForSession(
  app: WebdriverIO.Browser,
  path: string,
  expected: { boards: unknown[]; selected: number | null },
) {
  await app.waitUntil(
    () =>
      existsSync(path) &&
      isDeepStrictEqual(JSON.parse(readFileSync(path, "utf8")), expected),
    {
      timeoutMsg: `The board session did not reach ${JSON.stringify(expected)}`,
    },
  );
}
