import { ask, finishRequests, prepareToClose } from "./ipc";
import type { Change, Shown } from "./tabs";

/** Wait for the player's board actions to settle before sending the current tab order. */
export function boardSession(
  shown: () => Shown,
  report: (problem: string | undefined) => void,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
  };
  const write = (type: "CheckpointBoards" | "QuitWithBoards") => {
    const { tabs, shownTab } = shown();
    return ask({
      type,
      boards: tabs.map(({ game }) => game),
      selected: shownTab ?? null,
    });
  };
  return {
    changed(before: Shown, change: Change) {
      const after = shown();
      if (change.type === "RestoreBoards") return;
      if (
        before.shownTab === after.shownTab &&
        before.tabs.length === after.tabs.length
      ) {
        if (change.type === "drawPlace") {
          const previous = before.tabs.find(({ game }) => game === change.game);
          if (previous?.picture.place.join() === change.picture.place.join())
            return;
        } else if (
          change.type !== "drawPicture" &&
          change.type !== "updateGameFile"
        ) {
          return;
        }
      }
      cancel();
      const turn = setTimeout(async () => {
        await finishRequests();
        if (timer !== turn) return;
        let problem: string | undefined;
        try {
          await write("CheckpointBoards");
        } catch (error) {
          problem = `Could not save your latest board changes for restart. ${String(error)}`;
        }
        if (timer === turn) {
          timer = undefined;
          report(problem);
        }
      }, 400);
      timer = turn;
    },
    async quit(
      secondClose: AbortSignal,
      showWaiting: (message: string | null) => void,
    ) {
      cancel();
      await prepareToClose(secondClose, showWaiting);
      // Finishing an edit can schedule another notification before the quit takes its snapshot.
      cancel();
      await write("QuitWithBoards");
    },
    cancel,
  };
}
