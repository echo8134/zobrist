import { expect, test, vi } from "vitest";
import type { GamePicture } from "./generated/GamePicture";
import { ask } from "./ipc";
import { apply, type Change, closeTab, openTab, type Shown } from "./tabs";

vi.mock("./ipc", () => ({ ask: vi.fn() }));

const picture: GamePicture = {
  fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
  turn: "white",
  inCheck: false,
  legalMoves: [],
  lastMove: null,
  place: [],
  notation: [],
};

test("a tab opens with the game that Rust answered and closes that game with itself", async () => {
  let shown: Shown = { tabs: [], page: "Home", shownTab: undefined };
  const change = (next: Change) => {
    shown = apply(shown, next);
  };
  vi.mocked(ask).mockResolvedValueOnce({ type: "NewGame", game: 7, picture });

  await openTab(change);

  expect(vi.mocked(ask).mock.calls).toEqual([[{ type: "NewGame" }]]);
  expect(shown).toEqual({
    tabs: [{ game: 7, picture, bottomSide: "white" }],
    page: "Home",
    shownTab: 7,
  });

  closeTab(7, change);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "NewGame" }],
    [{ type: "CloseGame", game: 7 }],
  ]);
  expect(shown).toEqual({ tabs: [], page: "Home", shownTab: undefined });
});

test("a turn changes the side at the bottom of one tab only", () => {
  const tabs = [
    { game: 1, picture, bottomSide: "white" },
    { game: 2, picture, bottomSide: "white" },
  ] as const;
  const shown: Shown = { tabs, page: "Home", shownTab: 2 };

  const turnedOnce = apply(shown, { type: "turnBoard", game: 2 });
  const turnedTwice = apply(turnedOnce, { type: "turnBoard", game: 2 });

  expect(turnedOnce.tabs.map((tab) => tab.bottomSide)).toEqual([
    "white",
    "black",
  ]);
  expect(turnedTwice.tabs.map((tab) => tab.bottomSide)).toEqual([
    "white",
    "white",
  ]);
});
