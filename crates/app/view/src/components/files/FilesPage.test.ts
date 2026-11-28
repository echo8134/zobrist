import { beforeEach, expect, test, vi } from "vitest";
import type { GamePicture } from "../../generated/GamePicture";
import { ask } from "../../ipc";
import { openGame } from "./FilesPage";

vi.mock("../../ipc", () => ({ ask: vi.fn(), errorText: vi.fn() }));

beforeEach(() => {
  vi.mocked(ask).mockReset();
});

// Files hands a picture on and reads none of its fields.
const picture = {
  fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
} as GamePicture;
const pictureAfterE4 = {
  fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
} as GamePicture;

test("a game of a file goes on a board, also a game that is open already", async () => {
  const openOnBoard = vi.fn();
  vi.mocked(ask).mockResolvedValueOnce({
    type: "OpenGame",
    answer: { type: "Game", game: 7, picture: pictureAfterE4 },
  });

  expect(await openGame("/games/club.pgn", 2, openOnBoard)).toBe(true);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "OpenGame", path: "/games/club.pgn", number: 2 }],
  ]);
  expect(openOnBoard.mock.calls).toEqual([
    [{ game: 7, picture: pictureAfterE4 }],
  ]);

  // Rust answers with the open game that exists.
  vi.mocked(ask).mockResolvedValueOnce({
    type: "OpenGame",
    answer: { type: "Game", game: 3, picture },
  });
  expect(await openGame("/games/club.pgn", 1, openOnBoard)).toBe(true);
  expect(openOnBoard.mock.calls).toEqual([
    [{ game: 7, picture: pictureAfterE4 }],
    [{ game: 3, picture }],
  ]);
});

test("a game that Rust does not open goes on no board", async () => {
  const openOnBoard = vi.fn();
  vi.mocked(ask).mockResolvedValueOnce({
    type: "OpenGame",
    answer: { type: "Refusal", reason: "noSuchGame", system: null },
  });

  expect(await openGame("/games/club.pgn", 9, openOnBoard)).toBe(false);
  expect(openOnBoard).not.toHaveBeenCalled();
});
