import { beforeEach, expect, test, vi } from "vitest";
import type { GamePicture } from "./generated/GamePicture";
import type { NotationMove } from "./generated/NotationMove";
import { ask } from "./ipc";
import {
  apply,
  type Change,
  closeTab,
  goTo,
  openTab,
  placeToAskFor,
  playMove,
  type Shown,
  setShapes,
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

test("a walk to a place goes to Rust, and its tab takes the picture of the answer", async () => {
  const { change, shown } = tabState({
    tabs: [{ game: 7, picture, bottomSide: "white" }],
    page: "Home",
    shownTab: 7,
  });
  vi.mocked(ask).mockResolvedValueOnce({
    type: "GoTo",
    picture: pictureAfterE4,
  });

  await goTo(7, [0], change);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "GoTo", game: 7, place: [0] }],
  ]);
  expect(shown().tabs).toEqual([
    { game: 7, picture: pictureAfterE4, bottomSide: "white" },
  ]);
});

const move = (
  text: string,
  place: number[],
  variations: NotationMove[][] = [],
) => ({ text, place, variations }) as NotationMove;
const notation = [
  move("e4", [0]),
  move(
    "e5",
    [0, 0],
    [[move("c5", [0, 1]), move("Nc3", [0, 1, 0], [[move("Nf3", [0, 1, 1])]])]],
  ),
  move("Nf3", [0, 0, 0]),
];

test("the left arrow leads one move back and stays at the start", () => {
  expect(placeToAskFor("ArrowLeft", { place: [0, 1, 1], notation })).toEqual([
    0, 1,
  ]);
  expect(placeToAskFor("ArrowLeft", { place: [], notation })).toEqual([]);
});

test("the right arrow leads to the first move after the current one", () => {
  expect(placeToAskFor("ArrowRight", { place: [0], notation })).toEqual([0, 0]);
  expect(placeToAskFor("ArrowRight", { place: [0, 1], notation })).toEqual([
    0, 1, 0,
  ]);
});

test("Home leads to the start of the game", () => {
  expect(placeToAskFor("Home", { place: [0, 1, 0], notation })).toEqual([]);
});

test("End leads to the last move of the line that holds the current move", () => {
  expect(placeToAskFor("End", { place: [0], notation })).toEqual([0, 0, 0]);
  expect(placeToAskFor("End", { place: [0, 1], notation })).toEqual([0, 1, 0]);
  expect(placeToAskFor("End", { place: [0, 1, 1], notation })).toEqual([
    0, 1, 1,
  ]);
});

test("End leads from the start to the last move of the main line", () => {
  expect(placeToAskFor("End", { place: [], notation })).toEqual([0, 0, 0]);
  expect(placeToAskFor("End", { place: [], notation: [] })).toEqual([]);
});

test("another key leads nowhere", () => {
  expect(placeToAskFor("a", { place: [0], notation })).toBeUndefined();
});

test("all shapes of the board go to Rust, and the tab takes the picture of the answer", async () => {
  const { change, shown } = tabState({
    tabs: [
      { game: 7, picture, bottomSide: "white" },
      { game: 8, picture, bottomSide: "white" },
    ],
    page: "Home",
    shownTab: 7,
  });
  const squares = [{ color: "green", square: "d4" }] as const;
  const arrows = [{ color: "red", from: "d1", to: "h5" }] as const;
  const withShapes = { ...picture, squares, arrows } as unknown as GamePicture;
  vi.mocked(ask).mockResolvedValueOnce({
    type: "SetShapes",
    picture: withShapes,
  });

  await setShapes(7, { squares: [...squares], arrows: [...arrows] }, change);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "SetShapes", game: 7, squares, arrows }],
  ]);
  expect(shown().tabs).toEqual([
    { game: 7, picture: withShapes, bottomSide: "white" },
    { game: 8, picture, bottomSide: "white" },
  ]);

  vi.mocked(ask).mockResolvedValueOnce({ type: "SetShapes", picture: null });
  await setShapes(9, { squares: [], arrows: [] }, change);
  expect(shown().tabs[0]?.picture).toBe(withShapes);
});
