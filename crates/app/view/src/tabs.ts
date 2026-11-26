import type { GamePicture } from "./generated/GamePicture";
import type { Promotion } from "./generated/Promotion";
import type { Side } from "./generated/Side";
import { ask } from "./ipc";
import type { Page } from "./PageArea";

export const tabName = "New game";

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

export async function playMove(
  game: number,
  move: Move,
  change: (change: Change) => void,
) {
  const { picture } = await ask({ type: "PlayMove", game, ...move });
  if (picture !== null) {
    change({ type: "drawPicture", game, picture });
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

export function closeTab(game: number, change: (change: Change) => void) {
  change({ type: "closeTab", game });
  ask({ type: "CloseGame", game });
}
