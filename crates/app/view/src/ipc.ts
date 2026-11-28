import { invoke } from "@tauri-apps/api/core";
import type { DesktopRequest as Request } from "./generated/DesktopRequest";
import type { Response } from "./generated/Response";

const onTheirWay = new Map<number | undefined, Promise<void>>();

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
  const game = "game" in request ? request.game : undefined;
  // A fetched link joins the session when its answer opens the tab.
  const changesBoards =
    game === undefined
      ? request.type === "NewGame" ||
        request.type === "OpenPgnText" ||
        request.type === "OpenGame" ||
        request.type === "DeletePgnGame" ||
        request.type === "CheckpointBoards"
      : request.type !== "GamePgn" && request.type !== "Analysis";
  if (!changesBoards) return answer;
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
 * The board changes for a game that are still on their way, or nothing when none is. The promise
 * resolves when Rust has answered every board change for the game that was sent so far, and the
 * senders have handled those answers.
 *
 * Rust answers each request in a task of its own, so two requests that follow each other
 * closely can reach a game in the other order. A save waits here, or it could write the game
 * without the change that the player made right before it.
 */
export function answersOnTheirWay(game: number) {
  return onTheirWay.get(game);
}

/** Waits for pending board changes and session writes, including those their answers start. */
export async function finishBoardActions() {
  // Chessground delivers a completed move in a timer after the mouse event.
  await new Promise<void>((resolve) => setTimeout(resolve, 1));
  while (onTheirWay.size > 0) {
    await Promise.all(onTheirWay.values());
  }
}
