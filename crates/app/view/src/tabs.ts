import type { EditableTag } from "./generated/EditableTag";
import type { GamePicture } from "./generated/GamePicture";
import type { GameResult } from "./generated/GameResult";
import type { NotationMove } from "./generated/NotationMove";
import type { Promotion } from "./generated/Promotion";
import type { Side } from "./generated/Side";
import { ask } from "./ipc";
import type { Page } from "./PageArea";

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

export type Tab = {
  game: number;
  picture: GamePicture;
  bottomSide: Side;
};

export type Shown = {
  tabs: readonly Tab[];
  page: Page;
  shownTab: Tab["game"] | undefined;
};

export type Move = { from: string; to: string; promotion: Promotion | null };

/** The marked squares and the arrows of one place of a game. */
export type Shapes = Pick<GamePicture, "squares" | "arrows">;

export type Change =
  | { type: "openTab"; game: number; picture: GamePicture }
  | { type: "showTab"; game: number }
  | { type: "turnBoard"; game: number }
  | { type: "drawPicture"; game: number; picture: GamePicture }
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

export function apply(shown: Shown, change: Change): Shown {
  switch (change.type) {
    case "openTab":
      return {
        ...shown,
        tabs: [
          ...shown.tabs,
          { game: change.game, picture: change.picture, bottomSide: "white" },
        ],
        shownTab: change.game,
      };
    case "showTab":
      return { ...shown, shownTab: change.game };
    case "turnBoard":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        bottomSide: tab.bottomSide === "white" ? "black" : "white",
      }));
    case "drawPicture":
      return withChangedTab(shown, change.game, (tab) => ({
        ...tab,
        picture: change.picture,
      }));
    case "closeTab": {
      const place = shown.tabs.findIndex((tab) => tab.game === change.game);
      const tabAtRight = shown.tabs[place + 1];
      const tabAtLeft = shown.tabs[place - 1];
      return {
        ...shown,
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

async function drawAnswer(
  game: number,
  answer: Promise<{ picture: GamePicture | null }>,
  change: (change: Change) => void,
) {
  const { picture } = await answer;
  if (picture !== null) {
    change({ type: "drawPicture", game, picture });
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
  return drawAnswer(game, ask({ type: "GoTo", game, place }), change);
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

export function closeTab(game: number, change: (change: Change) => void) {
  change({ type: "closeTab", game });
  ask({ type: "CloseGame", game });
}
