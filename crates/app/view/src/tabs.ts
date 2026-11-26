import type { GamePicture } from "./generated/GamePicture";
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

export type Change =
  | { type: "openTab"; game: number; picture: GamePicture }
  | { type: "showTab"; game: number }
  | { type: "turnBoard"; game: number }
  | { type: "closeTab"; game: number }
  | { type: "showPage"; page: Page };

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
      return {
        ...shown,
        tabs: shown.tabs.map((tab) =>
          tab.game === change.game
            ? {
                ...tab,
                bottomSide: tab.bottomSide === "white" ? "black" : "white",
              }
            : tab,
        ),
      };
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

export function closeTab(game: number, change: (change: Change) => void) {
  change({ type: "closeTab", game });
  ask({ type: "CloseGame", game });
}
