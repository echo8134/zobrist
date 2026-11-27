import { beforeEach, expect, test, vi } from "vitest";
import type { GamePicture } from "./generated/GamePicture";
import type { NotationMove } from "./generated/NotationMove";
import { answersOnTheirWay, ask } from "./ipc";
import {
  annotate,
  apply,
  askForTool,
  type Change,
  closeOrAsk,
  closeTab,
  goTo,
  isSaveKey,
  openGame,
  openTab,
  placeToAskFor,
  playMove,
  refusedSaveLine,
  type Shown,
  save,
  setResult,
  setShapes,
  setTag,
  tabName,
  toolForKey,
} from "./tabs";

vi.mock("./ipc", () => ({ ask: vi.fn(), answersOnTheirWay: vi.fn() }));

beforeEach(() => {
  vi.mocked(ask).mockReset();
  vi.mocked(answersOnTheirWay).mockReset();
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
    tabs: [{ game: 7, picture, bottomSide: "white", gameData: "closed" }],
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
    { game: 1, picture, bottomSide: "white", gameData: "closed" },
    { game: 2, picture, bottomSide: "white", gameData: "closed" },
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
      { game: 7, picture, bottomSide: "black", gameData: "closed" },
      { game: 8, picture, bottomSide: "white", gameData: "closed" },
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
      {
        game: 7,
        picture: pictureAfterE4,
        bottomSide: "black",
        gameData: "closed",
      },
      { game: 8, picture, bottomSide: "white", gameData: "closed" },
    ],
    page: "Home",
    shownTab: 7,
  });
});

test("an answer without a picture changes no tab", async () => {
  const atStart: Shown = {
    tabs: [{ game: 8, picture, bottomSide: "white", gameData: "closed" }],
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
    tabs: [{ game: 7, picture, bottomSide: "white", gameData: "closed" }],
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
    {
      game: 7,
      picture: pictureAfterE4,
      bottomSide: "white",
      gameData: "closed",
    },
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

const withPlayers = (white: string | undefined, black: string | undefined) =>
  ({
    tags: [
      { name: "Event", value: "Example" },
      ...(white === undefined ? [] : [{ name: "White", value: white }]),
      ...(black === undefined ? [] : [{ name: "Black", value: black }]),
    ],
  }) as GamePicture;

test("a tab has the names of both players, and the name New game until both are set", () => {
  expect(tabName(withPlayers("Alpha", "Beta"))).toBe("Alpha - Beta");
  expect(tabName(withPlayers("Alpha", undefined))).toBe("New game");
  expect(tabName(withPlayers(undefined, "Beta"))).toBe("New game");
  expect(tabName(withPlayers(undefined, undefined))).toBe("New game");
});

test("a text of the game data goes to Rust, and the answer says whether Rust took it", async () => {
  const atStart = withPlayers("Alpha", undefined);
  const tab = {
    game: 7,
    picture: atStart,
    bottomSide: "white",
    gameData: "closed",
  } as const;
  const { change, shown } = tabState({
    tabs: [tab],
    page: "Home",
    shownTab: 7,
  });
  const taken = withPlayers("Alpha", "Beta");
  vi.mocked(ask).mockResolvedValueOnce({ type: "SetTag", picture: taken });

  expect(await setTag(tab, "Black", "Beta", change)).toBe(true);
  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "SetTag", game: 7, tag: "Black", text: "Beta" }],
  ]);
  expect(shown().tabs[0]?.picture).toBe(taken);

  // Rust answers a text that it refused with the tag as it was.
  vi.mocked(ask).mockResolvedValueOnce({ type: "SetTag", picture: atStart });
  expect(await setTag(tab, "Date", "26.11.2026", change)).toBe(false);
  expect(shown().tabs[0]?.picture).toBe(atStart);

  vi.mocked(ask).mockResolvedValueOnce({ type: "SetTag", picture: null });
  expect(await setTag(tab, "Date", "2026.11.26", change)).toBe(true);
  expect(shown().tabs[0]?.picture).toBe(atStart);
});

test("a result goes to Rust, and the tab takes the picture of the answer", async () => {
  const { change, shown } = tabState({
    tabs: [{ game: 7, picture, bottomSide: "white", gameData: "closed" }],
    page: "Home",
    shownTab: 7,
  });
  const won = { ...picture, result: "whiteWin" } as GamePicture;
  vi.mocked(ask).mockResolvedValueOnce({ type: "SetResult", picture: won });

  await setResult(7, "whiteWin", change);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "SetResult", game: 7, result: "whiteWin" }],
  ]);
  expect(shown().tabs[0]?.picture).toBe(won);
});

test("all shapes of the board go to Rust, and the tab takes the picture of the answer", async () => {
  const { change, shown } = tabState({
    tabs: [
      { game: 7, picture, bottomSide: "white", gameData: "closed" },
      { game: 8, picture, bottomSide: "white", gameData: "closed" },
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
    { game: 7, picture: withShapes, bottomSide: "white", gameData: "closed" },
    { game: 8, picture, bottomSide: "white", gameData: "closed" },
  ]);

  vi.mocked(ask).mockResolvedValueOnce({ type: "SetShapes", picture: null });
  await setShapes(9, { squares: [], arrows: [] }, change);
  expect(shown().tabs[0]?.picture).toBe(withShapes);
});

test.each([
  "Undo",
  "Redo",
  "PromoteVariation",
  "DeleteVariation",
  "DeleteRest",
] as const)(
  "the tool %s goes to Rust, and its tab takes the picture of the answer",
  async (tool) => {
    const { change, shown } = tabState({
      tabs: [{ game: 7, picture, bottomSide: "white", gameData: "closed" }],
      page: "Home",
      shownTab: 7,
    });
    vi.mocked(ask).mockResolvedValueOnce({
      type: tool,
      picture: pictureAfterE4,
    });

    await askForTool(7, tool, change);

    expect(vi.mocked(ask).mock.calls).toEqual([[{ type: tool, game: 7 }]]);
    expect(shown().tabs).toEqual([
      {
        game: 7,
        picture: pictureAfterE4,
        bottomSide: "white",
        gameData: "closed",
      },
    ]);
  },
);

test("Ctrl+Z is the key of Undo", () => {
  expect(toolForKey({ key: "z", ctrlKey: true, shiftKey: false })).toBe("Undo");
});

test("Ctrl+Y and Ctrl+Shift+Z are the keys of Redo", () => {
  expect(toolForKey({ key: "y", ctrlKey: true, shiftKey: false })).toBe("Redo");
  expect(toolForKey({ key: "Z", ctrlKey: true, shiftKey: true })).toBe("Redo");
});

test("a key without Ctrl and another key with Ctrl are the key of no tool", () => {
  expect(
    toolForKey({ key: "z", ctrlKey: false, shiftKey: false }),
  ).toBeUndefined();
  expect(
    toolForKey({ key: "a", ctrlKey: true, shiftKey: false }),
  ).toBeUndefined();
  expect(
    toolForKey({ key: "Y", ctrlKey: true, shiftKey: true }),
  ).toBeUndefined();
});

test.each([
  { type: "SetComment", text: "Open game" },
  { type: "SetMoveSymbol", symbol: "!?" },
  { type: "SetMoveSymbol", symbol: null },
  { type: "SetPositionSymbol", symbol: "⩲" },
  { type: "SetPositionSymbol", symbol: null },
] as const)(
  "the annotation $type goes to Rust with its value, and its tab takes the picture of the answer",
  async (annotation) => {
    const { change, shown } = tabState({
      tabs: [{ game: 7, picture, bottomSide: "white", gameData: "closed" }],
      page: "Home",
      shownTab: 7,
    });
    vi.mocked(ask).mockResolvedValueOnce({
      type: annotation.type,
      picture: pictureAfterE4,
    });

    await annotate(7, annotation, change);

    expect(vi.mocked(ask).mock.calls).toEqual([[{ ...annotation, game: 7 }]]);
    expect(shown().tabs).toEqual([
      {
        game: 7,
        picture: pictureAfterE4,
        bottomSide: "white",
        gameData: "closed",
      },
    ]);
  },
);

test("a game of a file opens in a tab, and a game that is open already gets no second tab", async () => {
  const { change, shown } = tabState({
    tabs: [{ game: 3, picture, bottomSide: "black", gameData: "closed" }],
    page: "Files",
    shownTab: undefined,
  });
  vi.mocked(ask).mockResolvedValueOnce({
    type: "OpenGame",
    answer: { type: "Game", game: 7, picture: pictureAfterE4 },
  });

  expect(await openGame("/games/club.pgn", 2, change)).toBe(true);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "OpenGame", path: "/games/club.pgn", number: 2 }],
  ]);
  expect(shown()).toEqual({
    tabs: [
      { game: 3, picture, bottomSide: "black", gameData: "closed" },
      {
        game: 7,
        picture: pictureAfterE4,
        bottomSide: "white",
        gameData: "closed",
      },
    ],
    page: "Files",
    shownTab: 7,
  });

  // Rust answers with the open game that exists.
  vi.mocked(ask).mockResolvedValueOnce({
    type: "OpenGame",
    answer: { type: "Game", game: 3, picture },
  });
  expect(await openGame("/games/club.pgn", 1, change)).toBe(true);
  expect(shown()).toEqual({
    tabs: [
      { game: 3, picture, bottomSide: "black", gameData: "closed" },
      {
        game: 7,
        picture: pictureAfterE4,
        bottomSide: "white",
        gameData: "closed",
      },
    ],
    page: "Files",
    shownTab: 3,
  });
});

test("a game that Rust does not open changes nothing", async () => {
  const atStart: Shown = { tabs: [], page: "Files", shownTab: undefined };
  const { change, shown } = tabState(atStart);
  vi.mocked(ask).mockResolvedValueOnce({
    type: "OpenGame",
    answer: { type: "Refusal", reason: "noSuchGame", system: null },
  });

  expect(await openGame("/games/club.pgn", 9, change)).toBe(false);
  expect(shown()).toEqual(atStart);
});

const inClub = { path: "/games/club.pgn", number: 2 };
const changedFromFile = {
  ...picture,
  file: inClub,
  unsaved: true,
} as GamePicture;
const savedInFile = { ...picture, file: inClub, unsaved: false } as GamePicture;
const changedWithoutFile = {
  ...picture,
  file: null,
  unsaved: true,
} as GamePicture;
const emptyWithoutFile = {
  ...picture,
  file: null,
  unsaved: false,
} as GamePicture;

function oneTab(ofTheTab: GamePicture) {
  const state = tabState({
    tabs: [
      { game: 7, picture: ofTheTab, bottomSide: "white", gameData: "closed" },
    ],
    page: "Home",
    shownTab: 7,
  });
  const tabNow = (game: number) =>
    state.shown().tabs.find((tab) => tab.game === game);
  return { ...state, tabNow };
}

test("Save writes a game from a file back into its place", async () => {
  const { change, shown, tabNow } = oneTab(changedFromFile);
  vi.mocked(ask).mockResolvedValueOnce({
    type: "SaveGame",
    answer: { type: "Saved", picture: savedInFile },
  });

  expect(await save(7, "Save", "/games/other.pgn", tabNow, change)).toBe(true);

  expect(vi.mocked(ask).mock.calls).toEqual([[{ type: "SaveGame", game: 7 }]]);
  expect(shown().tabs).toEqual([
    { game: 7, picture: savedInFile, bottomSide: "white", gameData: "closed" },
  ]);
});

test.each(["Save", "SaveAsNewGame"] as const)(
  "%s of a new game asks for its data before adding it to the file of the page Files",
  async (tool) => {
    const { change, shown, tabNow } = oneTab(changedWithoutFile);
    expect(await save(7, tool, "/games/club.pgn", tabNow, change)).toBe(false);
    expect(shown().tabs[0]?.gameData).toBe("openToSave");
    expect(vi.mocked(ask)).not.toHaveBeenCalled();

    vi.mocked(ask).mockResolvedValueOnce({
      type: "SaveGameAs",
      answer: { type: "Saved", picture: savedInFile },
    });

    expect(
      await save(7, "ConfirmSave", "/games/club.pgn", tabNow, change),
    ).toBe(true);

    expect(vi.mocked(ask).mock.calls).toEqual([
      [{ type: "SaveGameAs", game: 7, path: "/games/club.pgn" }],
    ]);
    expect(shown().tabs[0]?.picture).toBe(savedInFile);
    expect(shown().tabs[0]?.gameData).toBe("closed");
  },
);

test("a refused first save keeps the form open to save and gives its reason", async () => {
  const { change, shown, tabNow } = oneTab(changedWithoutFile);
  await save(7, "Save", "/games/club.pgn", tabNow, change);
  vi.mocked(ask).mockResolvedValueOnce({
    type: "SaveGameAs",
    answer: { type: "Refusal", reason: "cannotWrite", system: null },
  });

  expect(await save(7, "ConfirmSave", "/games/club.pgn", tabNow, change)).toBe(
    false,
  );
  expect(shown().tabs).toEqual([
    {
      game: 7,
      picture: changedWithoutFile,
      bottomSide: "white",
      gameData: "openToSave",
      refusedSave: { type: "Refusal", reason: "cannotWrite", system: null },
    },
  ]);
});

test("two confirmations waiting on the same field save only one copy", async () => {
  const { change, tabNow } = oneTab(changedWithoutFile);
  const confirm = () =>
    save(7, "ConfirmSave", "/games/club.pgn", tabNow, change);
  await save(7, "Save", "/games/club.pgn", tabNow, change);
  let pending: Promise<void> | undefined = Promise.resolve().then(() => {
    pending = undefined;
  });
  vi.mocked(answersOnTheirWay).mockImplementation(() => pending);
  vi.mocked(ask).mockImplementationOnce(async () => {
    pending = new Promise((resolve) => setTimeout(resolve)).then(() => {
      pending = undefined;
    });
    return {
      type: "SaveGameAs",
      answer: { type: "Saved", picture: savedInFile },
    };
  });

  expect(await Promise.all([confirm(), confirm()])).toEqual([true, false]);
  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "SaveGameAs", game: 7, path: "/games/club.pgn" }],
  ]);
});

test("Save as new game adds a game to the file that it came from", async () => {
  const { change, shown, tabNow } = oneTab(savedInFile);
  const asLastGame = {
    ...picture,
    file: { path: "/games/club.pgn", number: 3 },
    unsaved: false,
  } as GamePicture;
  vi.mocked(ask).mockResolvedValueOnce({
    type: "SaveGameAs",
    answer: { type: "Saved", picture: asLastGame },
  });

  expect(
    await save(7, "SaveAsNewGame", "/games/other.pgn", tabNow, change),
  ).toBe(true);

  expect(vi.mocked(ask).mock.calls).toEqual([
    [{ type: "SaveGameAs", game: 7, path: "/games/club.pgn" }],
  ]);
  expect(shown().tabs[0]?.picture).toBe(asLastGame);
});

test.each(["Save", "SaveAsNewGame"] as const)(
  "%s of a game without a file asks for a file on the page Files and saves nothing",
  async (tool) => {
    const { change, shown, tabNow } = oneTab(changedWithoutFile);

    expect(await save(7, tool, undefined, tabNow, change)).toBe(false);

    expect(vi.mocked(ask)).not.toHaveBeenCalled();
    expect(shown().tabs).toEqual([
      {
        game: 7,
        picture: changedWithoutFile,
        bottomSide: "white",
        gameData: "closed",
        refusedSave: { type: "Refusal", reason: "noFile", system: null },
      },
    ]);
  },
);

test("a game without changes is not saved, and an empty game is not saved as a new game", async () => {
  const fromFile = oneTab(savedInFile);
  await save(7, "Save", "/games/club.pgn", fromFile.tabNow, fromFile.change);

  const empty = oneTab(emptyWithoutFile);
  await save(7, "Save", "/games/club.pgn", empty.tabNow, empty.change);
  await save(7, "SaveAsNewGame", "/games/club.pgn", empty.tabNow, empty.change);

  const closed = oneTab(changedFromFile);
  await save(8, "Save", "/games/club.pgn", closed.tabNow, closed.change);

  expect(vi.mocked(ask)).not.toHaveBeenCalled();
  expect(empty.shown().tabs).toEqual([
    {
      game: 7,
      picture: emptyWithoutFile,
      bottomSide: "white",
      gameData: "closed",
    },
  ]);
});

test("a save takes the game as it is after the answers to the requests before it", async () => {
  const { change, shown, tabNow } = oneTab(savedInFile);
  // The change that the player made right before the save is drawn while the save waits.
  vi.mocked(answersOnTheirWay).mockReturnValueOnce(
    Promise.resolve().then(() =>
      change({ type: "drawPicture", game: 7, picture: changedFromFile }),
    ),
  );
  vi.mocked(ask).mockResolvedValueOnce({
    type: "SaveGame",
    answer: { type: "Saved", picture: savedInFile },
  });

  await save(7, "Save", undefined, tabNow, change);

  expect(vi.mocked(answersOnTheirWay).mock.calls).toEqual([[7], [7]]);
  expect(vi.mocked(ask).mock.calls).toEqual([[{ type: "SaveGame", game: 7 }]]);
  expect(shown().tabs[0]?.picture).toBe(savedInFile);
});

test("a refused save stays as a line until the game changes or a save works", async () => {
  const { change, shown, tabNow } = oneTab(changedFromFile);
  const refusal = {
    type: "Refusal",
    reason: "fileChanged",
    system: null,
  } as const;
  const refused = { type: "SaveGame", answer: refusal } as const;
  vi.mocked(ask).mockResolvedValueOnce(refused);

  expect(await save(7, "Save", undefined, tabNow, change)).toBe(false);

  expect(shown().tabs).toEqual([
    {
      game: 7,
      picture: changedFromFile,
      bottomSide: "white",
      gameData: "closed",
      refusedSave: refusal,
    },
  ]);

  // A walk through the game is no change of the game.
  vi.mocked(ask).mockResolvedValueOnce({
    type: "GoTo",
    picture: changedFromFile,
  });
  await goTo(7, [0], change);
  expect(shown().tabs[0]?.refusedSave).toEqual(refusal);

  vi.mocked(ask).mockResolvedValueOnce({
    type: "PlayMove",
    picture: changedFromFile,
  });
  await playMove(7, { from: "e2", to: "e4", promotion: null }, change);
  expect(shown().tabs[0]).not.toHaveProperty("refusedSave");

  vi.mocked(ask).mockResolvedValueOnce(refused);
  await save(7, "Save", undefined, tabNow, change);
  expect(shown().tabs[0]?.refusedSave).toEqual(refusal);

  vi.mocked(ask).mockResolvedValueOnce({
    type: "SaveGame",
    answer: { type: "Saved", picture: savedInFile },
  });
  await save(7, "Save", undefined, tabNow, change);
  expect(shown().tabs).toEqual([
    { game: 7, picture: savedInFile, bottomSide: "white", gameData: "closed" },
  ]);
  expect(shown().tabs[0]).not.toHaveProperty("refusedSave");
});

test("a save whose game is no longer open changes nothing", async () => {
  const { change, shown, tabNow } = oneTab(changedFromFile);
  vi.mocked(ask).mockResolvedValueOnce({ type: "SaveGame", answer: null });

  expect(await save(7, "Save", undefined, tabNow, change)).toBe(false);

  expect(shown().tabs).toEqual([
    {
      game: 7,
      picture: changedFromFile,
      bottomSide: "white",
      gameData: "closed",
    },
  ]);
});

test("the line of a refused save says what went wrong and what the system said", () => {
  expect(
    refusedSaveLine({ type: "Refusal", reason: "noFile", system: null }),
  ).toBe("Open a file on the page Files first.");
  expect(
    refusedSaveLine({ type: "Refusal", reason: "fileChanged", system: null }),
  ).toBe("The file changed since this game was opened. Save it as a new game.");
  expect(
    refusedSaveLine({
      type: "Refusal",
      reason: "cannotWrite",
      system: "Permission denied (os error 13)",
    }),
  ).toBe("The file cannot be written. Permission denied (os error 13)");
});

test("Ctrl+S is the key of Save", () => {
  expect(isSaveKey({ key: "s", ctrlKey: true, shiftKey: false })).toBe(true);
  expect(isSaveKey({ key: "S", ctrlKey: true, shiftKey: false })).toBe(true);
  expect(isSaveKey({ key: "s", ctrlKey: false, shiftKey: false })).toBe(false);
  expect(isSaveKey({ key: "S", ctrlKey: true, shiftKey: true })).toBe(false);
  expect(isSaveKey({ key: "z", ctrlKey: true, shiftKey: false })).toBe(false);
});

test("a tab without changes closes at once, and a tab with changes asks first", () => {
  const saved = oneTab(savedInFile);

  closeOrAsk(7, saved.tabNow, saved.change);

  expect(vi.mocked(answersOnTheirWay).mock.calls).toEqual([[7]]);
  expect(vi.mocked(ask).mock.calls).toEqual([[{ type: "CloseGame", game: 7 }]]);
  expect(saved.shown()).toEqual({
    tabs: [],
    page: "Home",
    shownTab: undefined,
  });

  const changed = oneTab(changedFromFile);

  closeOrAsk(7, changed.tabNow, changed.change);
  // A tab that is gone already has nothing to close and nothing to ask.
  closeOrAsk(8, changed.tabNow, changed.change);

  expect(vi.mocked(ask)).toHaveBeenCalledTimes(1);
  expect(changed.shown()).toEqual({
    tabs: [
      {
        game: 7,
        picture: changedFromFile,
        bottomSide: "white",
        gameData: "closed",
      },
    ],
    page: "Home",
    shownTab: 7,
    asksToClose: 7,
  });
});

test("a tab asks when a change of its game arrives while the close waits for it", async () => {
  const { change, shown, tabNow } = oneTab(savedInFile);
  // The change that the player made right before the click is drawn while the close waits.
  vi.mocked(answersOnTheirWay).mockReturnValueOnce(
    Promise.resolve().then(() =>
      change({ type: "drawPicture", game: 7, picture: changedFromFile }),
    ),
  );

  const closed = closeOrAsk(7, tabNow, change);

  expect(shown().tabs[0]?.picture).toBe(savedInFile);
  await closed;
  expect(vi.mocked(ask)).not.toHaveBeenCalled();
  expect(shown()).toEqual({
    tabs: [
      {
        game: 7,
        picture: changedFromFile,
        bottomSide: "white",
        gameData: "closed",
      },
    ],
    page: "Home",
    shownTab: 7,
    asksToClose: 7,
  });
});

test("the question ends with Keep, with the close of its tab, and when its game has no changes any more", () => {
  const asking: Shown = {
    tabs: [
      {
        game: 7,
        picture: changedFromFile,
        bottomSide: "white",
        gameData: "closed",
      },
      {
        game: 8,
        picture: changedWithoutFile,
        bottomSide: "white",
        gameData: "closed",
      },
    ],
    page: "Home",
    shownTab: 7,
    asksToClose: 7,
  };

  expect(apply(asking, { type: "keepTab" })).not.toHaveProperty("asksToClose");
  expect(apply(asking, { type: "closeTab", game: 7 })).toEqual({
    tabs: [
      {
        game: 8,
        picture: changedWithoutFile,
        bottomSide: "white",
        gameData: "closed",
      },
    ],
    page: "Home",
    shownTab: 8,
  });
  expect(
    apply(asking, { type: "drawPicture", game: 7, picture: savedInFile }),
  ).not.toHaveProperty("asksToClose");

  // Another tab and a game that still has changes leave the question.
  expect(apply(asking, { type: "closeTab", game: 8 }).asksToClose).toBe(7);
  expect(
    apply(asking, { type: "drawPicture", game: 8, picture: emptyWithoutFile })
      .asksToClose,
  ).toBe(7);
  expect(
    apply(asking, { type: "drawPicture", game: 7, picture: changedFromFile })
      .asksToClose,
  ).toBe(7);
});
