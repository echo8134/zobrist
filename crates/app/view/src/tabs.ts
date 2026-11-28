import type { EditableTag } from "./generated/EditableTag";
import type { GameFileRefusal } from "./generated/GameFileRefusal";
import type { GamePicture } from "./generated/GamePicture";
import type { GameResult } from "./generated/GameResult";
import type { MoveSymbol } from "./generated/MoveSymbol";
import type { NotationMove } from "./generated/NotationMove";
import type { PositionSymbol } from "./generated/PositionSymbol";
import type { Promotion } from "./generated/Promotion";
import type { SaveGameAnswer } from "./generated/SaveGameAnswer";
import type { Side } from "./generated/Side";
import { answersOnTheirWay, ask } from "./ipc";
import type { Page } from "./PageArea";
import type { PositionSetup } from "./positionSetup";

export type BoardPlace = Pick<GamePicture, "revision" | "place">;

export function sameBoardPlace(left: BoardPlace, right: BoardPlace) {
  return (
    left.revision === right.revision && left.place.join() === right.place.join()
  );
}

/** The value of a tag of the game, or the empty text when the game has no such tag. */
export function tagValue(picture: GamePicture, name: EditableTag) {
  return picture.tags.find((tag) => tag.name === name)?.value ?? "";
}

/** The names of the two players as soon as the game has both. */
export function tabName(picture: GamePicture) {
  const white = tagValue(picture, "White");
  const black = tagValue(picture, "Black");
  return white !== "" && black !== "" ? `${white} - ${black}` : "New game";
}

/** Why Rust did not save a game. */
export type SaveRefusal = Extract<SaveGameAnswer, { type: "Refusal" }>;

export type PositionPanelState = (
  | { type: "openingSetup" }
  | { type: "setup"; draft: PositionSetup }
  | { type: "paste"; text: string }
) & { status?: "pending" | "refused" };

export type Tab = {
  game: number;
  picture: GamePicture;
  bottomSide: Side;
  panel?: PositionPanelState | undefined;
  openingProblem?: string | null | undefined;
  gameData: "closed" | "open" | "openToSave";
  /** The last save, when it was refused and the game has not changed since. */
  refusedSave?: SaveRefusal;
  saveWarning?: string | undefined;
};

export type Shown = {
  tabs: readonly Tab[];
  page: Page;
  shownTab: Tab["game"] | undefined;
  /** The tab that asks whether to close it with the changes of its game. */
  asksToClose?: Tab["game"];
};

export type Move = { from: string; to: string; promotion: Promotion | null };

/** The marked squares and the arrows of one place of a game. */
export type Shapes = Pick<GamePicture, "squares" | "arrows">;

export type Change =
  | {
      type: "openTab";
      game: number;
      picture: GamePicture;
      openingProblem?: string | null;
    }
  | { type: "setPanel"; game: number; panel: PositionPanelState | undefined }
  | { type: "showTab"; game: number }
  | { type: "turnBoard"; game: number }
  | { type: "setGameData"; game: number; gameData: Tab["gameData"] }
  | {
      type: "drawPicture";
      game: number;
      picture: GamePicture;
      saveWarning?: string | undefined;
      from?: BoardPlace;
    }
  | { type: "drawPlace"; game: number; picture: GamePicture }
  | { type: "refuseSave"; game: number; refusal: SaveRefusal }
  | { type: "askToClose"; game: number }
  | { type: "keepTab" }
  | { type: "closeTab"; game: number }
  | { type: "showPage"; page: Page };

function withChangedTab(
  shown: Shown,
  game: number,
  changed: (tab: Tab) => Tab,
): Shown {
  return {
    ...shown,
    tabs: shown.tabs.map((tab) => (tab.game === game ? changed(tab) : tab)),
  };
}

/** A tab asks only while it is open and its game has changes. */
function withoutQuestionOf(shown: Shown, game: number | undefined): Shown {
  if (shown.asksToClose !== game) {
    return shown;
  }
  const { asksToClose: _, ...rest } = shown;
  return rest;
}

export function apply(shown: Shown, change: Change): Shown {
  switch (change.type) {
    case "openTab":
      return {
        ...shown,
        // Rust answers a game of a file that is open already with that open game, and one
        // game has one tab.
        tabs: shown.tabs.some((tab) => tab.game === change.game)
          ? shown.tabs
          : [
              ...shown.tabs,
              {
                game: change.game,
                picture: change.picture,
                bottomSide: "white",
                openingProblem: change.openingProblem,
                gameData: "closed",
              },
            ],
        shownTab: change.game,
      };
    case "setPanel":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        panel: change.panel,
      }));
    case "showTab":
      return { ...shown, shownTab: change.game };
    case "setGameData":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        gameData: change.gameData,
      }));
    case "turnBoard":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        bottomSide: tab.bottomSide === "white" ? "black" : "white",
      }));
    case "drawPicture":
      if (change.from !== undefined) {
        const tab = shown.tabs.find(({ game }) => game === change.game);
        if (!tab || !sameBoardPlace(tab.picture, change.from)) return shown;
      }
      return withChangedTab(
        change.picture.unsaved ? shown : withoutQuestionOf(shown, change.game),
        change.game,
        ({ refusedSave: _, ...tab }) => ({
          ...tab,
          picture: change.picture,
          saveWarning: change.saveWarning,
        }),
      );
    case "drawPlace":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        picture: change.picture,
      }));
    case "refuseSave":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        refusedSave: change.refusal,
        saveWarning: undefined,
      }));
    case "askToClose":
      return { ...shown, asksToClose: change.game };
    case "keepTab":
      return withoutQuestionOf(shown, shown.asksToClose);
    case "closeTab": {
      const place = shown.tabs.findIndex((tab) => tab.game === change.game);
      const tabAtRight = shown.tabs[place + 1];
      const tabAtLeft = shown.tabs[place - 1];
      return {
        ...withoutQuestionOf(shown, change.game),
        tabs: shown.tabs.filter((tab) => tab.game !== change.game),
        shownTab:
          shown.shownTab === change.game
            ? (tabAtRight ?? tabAtLeft)?.game
            : shown.shownTab,
      };
    }
    case "showPage":
      return { ...shown, page: change.page, shownTab: undefined };
  }
}

export async function openTab(change: (change: Change) => void) {
  const { game, picture } = await ask({ type: "NewGame" });
  change({ type: "openTab", game, picture });
}

/**
 * Opens a game of a PGN file in a tab, or shows the tab that holds it already. Tells whether
 * Rust opened the game.
 */
export async function openGame(
  path: string,
  number: number,
  change: (change: Change) => void,
) {
  const { answer } = await ask({ type: "OpenGame", path, number });
  if (answer.type === "Refusal") {
    return false;
  }
  change({ type: "openTab", game: answer.game, picture: answer.picture });
  return true;
}

async function drawAnswer(
  game: number,
  answer: Promise<{ picture: GamePicture | null }>,
  change: (change: Change) => void,
  type: "drawPicture" | "drawPlace" = "drawPicture",
) {
  const { picture } = await answer;
  if (picture !== null) {
    change({ type, game, picture });
  }
}

export function playMove(
  game: number,
  move: Move,
  change: (change: Change) => void,
) {
  return drawAnswer(game, ask({ type: "PlayMove", game, ...move }), change);
}

export function goTo(
  game: number,
  place: number[],
  change: (change: Change) => void,
) {
  return drawAnswer(
    game,
    ask({ type: "GoTo", game, place }),
    change,
    "drawPlace",
  );
}

export type NotationTool =
  | "Undo"
  | "Redo"
  | "PromoteVariation"
  | "DeleteVariation"
  | "DeleteRest";

export function askForTool(
  game: number,
  tool: NotationTool,
  change: (change: Change) => void,
) {
  return drawAnswer(game, ask({ type: tool, game }), change);
}

export function toolForKey({
  key,
  ctrlKey,
  shiftKey,
}: Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey">):
  | NotationTool
  | undefined {
  const letter = ctrlKey ? key.toLowerCase() : undefined;
  if (letter === "z") {
    return shiftKey ? "Redo" : "Undo";
  }
  return letter === "y" && !shiftKey ? "Redo" : undefined;
}

function lineThatHolds(
  place: readonly number[],
  line: readonly NotationMove[],
): readonly NotationMove[] | undefined {
  for (const move of line) {
    if (move.place.join() === place.join()) {
      return line;
    }
    for (const variation of move.variations) {
      const found = lineThatHolds(place, variation);
      if (found !== undefined) {
        return found;
      }
    }
  }
  return undefined;
}

export function placeToAskFor(
  key: string,
  { place, notation }: Pick<GamePicture, "place" | "notation">,
): number[] | undefined {
  switch (key) {
    case "ArrowLeft":
      return place.slice(0, -1);
    case "ArrowRight":
      return [...place, 0];
    case "Home":
      return [];
    case "End": {
      const line =
        place.length === 0 ? notation : lineThatHolds(place, notation);
      return line?.at(-1)?.place ?? place;
    }
    default:
      return undefined;
  }
}

export type Annotation =
  | { type: "SetComment"; text: string }
  | { type: "SetMoveSymbol"; symbol: MoveSymbol | null }
  | { type: "SetPositionSymbol"; symbol: PositionSymbol | null };

export function annotate(
  game: number,
  annotation: Annotation,
  change: (change: Change) => void,
) {
  return drawAnswer(game, ask({ ...annotation, game }), change);
}

/** Sends all shapes that the board shows at the current place and draws the answer. */
export async function setShapes(
  game: number,
  shapes: Shapes,
  change: (change: Change) => void,
) {
  const { picture } = await ask({ type: "SetShapes", game, ...shapes });
  if (picture !== null) {
    change({ type: "drawPicture", game, picture });
  }
}

/**
 * Sends the text of one field of the game data and draws the answer. Tells whether Rust took
 * the text. A text that Rust refused leaves the tag of the picture as it was.
 */
export async function setTag(
  tab: Tab,
  tag: EditableTag,
  text: string,
  change: (change: Change) => void,
) {
  const { game } = tab;
  const { picture } = await ask({ type: "SetTag", game, tag, text });
  if (picture === null) {
    return true;
  }
  change({ type: "drawPicture", game, picture });
  return tagValue(picture, tag) !== tagValue(tab.picture, tag);
}

export async function setResult(
  game: number,
  result: GameResult,
  change: (change: Change) => void,
) {
  const { picture } = await ask({ type: "SetResult", game, result });
  if (picture !== null) {
    change({ type: "drawPicture", game, picture });
  }
}

export type SaveTool = "Save" | "SaveAsNewGame" | "ConfirmSave";

export function isSaveKey({
  key,
  ctrlKey,
  shiftKey,
}: Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey">) {
  return ctrlKey && !shiftKey && key.toLowerCase() === "s";
}

/**
 * Saves the game of a tab. Save writes a game from a file back into its place. A game without
 * a file, and every game that is saved as a new game, becomes the last game of a file: the
 * file that the game came from, or else the file of the page Files. Tells whether Rust saved
 * the game.
 */
export async function save(
  game: number,
  tool: SaveTool,
  fileOfThePage: string | undefined,
  tabNow: (game: number) => Tab | undefined,
  change: (change: Change) => void,
) {
  // Two confirmations can wait on the same field. When it arrives, the second must also
  // wait for the first save, or both would append the game.
  let pending = answersOnTheirWay(game);
  while (pending !== undefined) {
    await pending;
    pending = answersOnTheirWay(game);
  }
  const tab = tabNow(game);
  const picture = tab?.picture;
  // A game without a file and without changes is empty, and nothing saves it.
  if (
    picture === undefined ||
    (tool === "ConfirmSave" && tab?.gameData !== "openToSave") ||
    (!picture.unsaved && (tool === "Save" || picture.file === null))
  ) {
    return false;
  }
  const path = picture.file?.path ?? fileOfThePage;
  let answer: SaveGameAnswer | null;
  if (tool === "Save" && picture.file !== null) {
    ({ answer } = await ask({ type: "SaveGame", game }));
  } else if (path === undefined) {
    answer = { type: "Refusal", reason: "noFile", system: null };
  } else if (picture.file === null && tool !== "ConfirmSave") {
    change({ type: "setGameData", game, gameData: "openToSave" });
    return false;
  } else {
    ({ answer } = await ask({ type: "SaveGameAs", game, path }));
  }
  if (answer?.type === "Refusal") {
    change({ type: "refuseSave", game, refusal: answer });
  } else if (answer !== null) {
    change({
      type: "drawPicture",
      game,
      picture: answer.picture,
      saveWarning:
        answer.type === "SavedWithWarning" ? answer.warning : undefined,
    });
    if (tool === "ConfirmSave") {
      change({ type: "setGameData", game, gameData: "closed" });
    }
    return true;
  }
  return false;
}

const refusedSaves: Record<GameFileRefusal, string> = {
  cannotRead: "The file cannot be read.",
  noSuchGame: "The file has no game with this number.",
  noFile: "Open a file on the page Files first.",
  fileChanged:
    "The file changed since this game was opened. Save it as a new game.",
  changeInProgress: "Another change to this file is still running. Try again.",
  cannotReplace: "The game cannot be replaced in its file.",
  cannotWrite: "The file cannot be written.",
};

/** The line of a refused save, with what the system said behind it. */
export function refusedSaveLine({ reason, system }: SaveRefusal) {
  return system === null
    ? refusedSaves[reason]
    : `${refusedSaves[reason]} ${system}`;
}

export function closeTab(game: number, change: (change: Change) => void) {
  change({ type: "closeTab", game });
  ask({ type: "CloseGame", game });
}

/**
 * Closes the tab of a game without changes. A tab whose game has changes asks first. A change
 * that the player made right before counts, also while its answer is still on its way.
 */
export function closeOrAsk(
  game: number,
  tabNow: (game: number) => Tab | undefined,
  change: (change: Change) => void,
) {
  const decide = () => {
    const unsaved = tabNow(game)?.picture.unsaved;
    if (unsaved === true) {
      change({ type: "askToClose", game });
    } else if (unsaved === false) {
      closeTab(game, change);
    }
  };
  // Without an answer on its way the tab closes in the same moment, as a click expects.
  return answersOnTheirWay(game)?.then(decide) ?? decide();
}
