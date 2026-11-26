import { beforeEach, expect, test, vi } from "vitest";
import type { GamePicture } from "./generated/GamePicture";
import { ask } from "./ipc";
import {
  apply,
  type Change,
  closeTab,
  openTab,
  playMove,
  type Shown,
} from "./tabs";

vi.mock("./ipc", () => ({ ask: vi.fn() }));

beforeEach(() => {
  vi.mocked(ask).mockReset();
});

// The tab state hands a picture on and reads none of its fields. With one field here, a new
// field of the picture in Rust does not break this test.
const picture = {
  fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
} as GamePicture;
const pictureAfterE4 = {
  fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
} as GamePicture;

function tabState(atStart: Shown) {
  let shown = atStart;
  return {
    change: (next: Change) => {
      shown = apply(shown, next);
    },
    shown: () => shown,
  };
}

test("a tab opens with the game that Rust answered and closes that game with itself", async () => {
  const { change, shown } = tabState({
    tabs: [],
    page: "Home",
    shownTab: undefined,
  });
  vi.mocked(ask).mockResolvedValueOnce({ type: "NewGame", game: 7, picture });

  await openTab(change);

  expect(vi.mocked(ask).mock.calls).toEqual([[{ type: "NewGame" }]]);
  expect(shown()).toEqual({
    tabs: [{ game: 7, picture, bottomSide: "white" }],
    page: "Home",
    shownTab: 7,
  });

  closeTab(7, change);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "NewGame" }],
    [{ type: "CloseGame", game: 7 }],
  ]);
  expect(shown()).toEqual({ tabs: [], page: "Home", shownTab: undefined });
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

test("a move goes to Rust, and its tab takes the picture of the answer", async () => {
  const { change, shown } = tabState({
    tabs: [
      { game: 7, picture, bottomSide: "black" },
      { game: 8, picture, bottomSide: "white" },
    ],
    page: "Home",
    shownTab: 7,
  });
  vi.mocked(ask).mockResolvedValueOnce({
    type: "PlayMove",
    picture: pictureAfterE4,
  });

  await playMove(7, { from: "e2", to: "e4", promotion: null }, change);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "PlayMove", game: 7, from: "e2", to: "e4", promotion: null }],
  ]);
  expect(shown()).toEqual({
    tabs: [
      { game: 7, picture: pictureAfterE4, bottomSide: "black" },
      { game: 8, picture, bottomSide: "white" },
    ],
    page: "Home",
    shownTab: 7,
  });
});

test("an answer without a picture changes no tab", async () => {
  const atStart: Shown = {
    tabs: [{ game: 8, picture, bottomSide: "white" }],
    page: "Home",
    shownTab: 8,
  };
  const { change, shown } = tabState(atStart);
  vi.mocked(ask).mockResolvedValueOnce({ type: "PlayMove", picture: null });

  await playMove(7, { from: "e2", to: "e4", promotion: null }, change);

  expect(shown()).toEqual(atStart);
});
