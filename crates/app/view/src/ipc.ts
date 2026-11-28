import { invoke } from "@tauri-apps/api/core";
import type { DesktopRequest as Request } from "./generated/DesktopRequest";
import type { Response } from "./generated/Response";

const onTheirWay = new Map<Promise<void>, number | "boards" | "background">();
const longRequests = new Set<Promise<unknown>>();

export function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

/** Asks Rust. The answer has the type of the response case that is named like the request. */
export function ask<Asked extends Request>(
  request: Asked,
): Promise<Extract<Response, { type: Asked["type"] }>> {
  const answer = invoke<Extract<Response, { type: Asked["type"] }>>("answer", {
    request,
  });
  // A close query must not count itself among the requests it is checking.
  if (request.type === "PrepareToClose") return answer;
  // A read-only lookup can outlive a save or the board that requested it.
  if (request.type === "LookupTablebase") return answer;
  if (
    request.type === "ImportDatabase" ||
    request.type === "DownloadLichessGames"
  ) {
    const pending = answer.finally(() => longRequests.delete(pending));
    longRequests.add(pending);
    return pending;
  }
  const game = "game" in request ? request.game : undefined;
  // A fetched link joins the session when its answer opens the tab.
  const changesBoards =
    game === undefined
      ? request.type === "NewGame" ||
        request.type === "OpenPgnText" ||
        request.type === "OpenGame" ||
        request.type === "OpenDatabaseGame" ||
        request.type === "DeletePgnGame" ||
        request.type === "CheckpointBoards"
      : request.type !== "GamePgn" && request.type !== "Analysis";
  const handled: Promise<void> = Promise.allSettled([answer])
    // A sender handles its answer in the same turn of the event loop in which it came.
    .then(() => new Promise<void>((nextTurn) => setTimeout(nextTurn)))
    .then(() => {
      onTheirWay.delete(handled);
    });
  onTheirWay.set(handled, changesBoards ? (game ?? "boards") : "background");
  return answer;
}

/**
 * A game save waits for edits to that game. A checkpoint waits for all board actions.
 * The senders need a turn to apply their answers before the view can take its snapshot.
 */
export function answersOnTheirWay(
  scope: number | "boards" | "all",
): Promise<void> | undefined {
  const pending: Promise<unknown>[] = [];
  if (scope === "all" && longRequests.size)
    pending.push(Promise.allSettled(longRequests));
  for (const [answer, action] of onTheirWay) {
    if (
      scope === "all" ||
      scope === action ||
      (scope === "boards" && action !== "background")
    ) {
      pending.push(answer);
    }
  }
  return pending.length
    ? Promise.all(pending).then(() => undefined)
    : undefined;
}

/** Include requests that the senders start when they handle their answers. */
export async function finishRequests(scope: "boards" | "all") {
  // Chessground delivers a completed move in a timer after the mouse event.
  await new Promise<void>((resolve) => setTimeout(resolve, 1));
  for (;;) {
    const pending = answersOnTheirWay(scope);
    if (!pending) return;
    await pending;
  }
}

/** Shows Rust's reason for waiting and gives up the tasks if the window is closed again. */
export async function prepareToClose(
  secondClose: AbortSignal,
  showWaiting: (message: string | null) => void,
) {
  const repeated = new Promise<void>((resolve) => {
    if (secondClose.aborted) resolve();
    else secondClose.addEventListener("abort", () => resolve(), { once: true });
  });
  async function waitForTasks() {
    // Chessground delivers a completed move in a timer after the mouse event.
    await new Promise<void>((resolve) => setTimeout(resolve, 1));
    while (!secondClose.aborted) {
      const { message } = await ask({ type: "PrepareToClose", abandon: false });
      if (secondClose.aborted) return;
      showWaiting(message);
      if (message === null && longRequests.size === 0) {
        if (onTheirWay.size === 0) return;
        // A picker can start a long task while another request is still pending.
        await Promise.race([repeated, ...onTheirWay.keys()]);
        continue;
      }
      // The close request can reach Rust before the request that started the task.
      const changed =
        message === null || longRequests.size === 0
          ? new Promise<void>((resolve) => setTimeout(resolve, 50))
          : Promise.race(longRequests).catch(() => {});
      await Promise.race([changed, repeated]);
    }
  }
  await Promise.race([waitForTasks(), repeated]);
  if (secondClose.aborted) {
    await ask({ type: "PrepareToClose", abandon: true });
    longRequests.clear();
    await finishRequests("boards");
  }
}
