import { invoke } from "@tauri-apps/api/core";
import type { DesktopRequest as Request } from "./generated/DesktopRequest";
import type { Response } from "./generated/Response";

const onTheirWay = new Map<Promise<void>, number | "boards" | "background">();

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
 * A quit also waits for unrelated work such as an import. In each case the senders need
 * a turn to apply their answers before the view can take its snapshot.
 */
export function answersOnTheirWay(
  scope: number | "boards" | "all",
): Promise<void> | undefined {
  const pending = [];
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
