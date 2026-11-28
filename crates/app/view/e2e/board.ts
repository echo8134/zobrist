import { browser } from "@wdio/globals";

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
