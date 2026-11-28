import { invoke } from "@tauri-apps/api/core";
import type { DesktopRequest as Request } from "./generated/DesktopRequest";
import type { Response } from "./generated/Response";

// Requests without a game include the ones that open a new board.
const onTheirWay = new Map<number | undefined, Promise<void>>();
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
  const handled: Promise<void> = Promise.allSettled([
    onTheirWay.get(game),
    answer,
  ])
    // A sender handles its answer in the same turn of the event loop in which it came.
    .then(() => new Promise<void>((nextTurn) => setTimeout(nextTurn)))
    .then(() => {
      if (onTheirWay.get(game) === handled) {
        onTheirWay.delete(game);
      }
    });
  onTheirWay.set(game, handled);
  return answer;
}

/**
 * The answers about a game that are still on their way, or nothing when none is. The promise
 * resolves when Rust has answered every request about the game that was sent so far, and the
 * senders have handled those answers.
 *
 * Rust answers each request in a task of its own, so two requests that follow each other
 * closely can reach a game in the other order. A save waits here, or it could write the game
 * without the change that the player made right before it.
 */
export function answersOnTheirWay(game: number) {
  return onTheirWay.get(game);
}

/** Waits for pending actions, including requests that their answers start. */
export async function finishBoardActions() {
  // Chessground delivers a completed move in a timer after the mouse event.
  await new Promise<void>((resolve) => setTimeout(resolve, 1));
  while (onTheirWay.size > 0) {
    await Promise.all(onTheirWay.values());
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
    for (;;) {
      // A pending picker can start an import or download when its answer arrives.
      await finishBoardActions();
      if (secondClose.aborted) return;
      const { message } = await ask({ type: "PrepareToClose", abandon: false });
      if (secondClose.aborted || (message === null && longRequests.size === 0))
        return;
      showWaiting(message);
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
    await finishBoardActions();
  }
}
