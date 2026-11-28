import { useState } from "react";
import type { GamePicture } from "./generated/GamePicture";
import type { TablebaseAnswer } from "./generated/TablebaseAnswer";
import { ask } from "./ipc";
import styles from "./Tablebase.module.css";

const { panel, controls, refusal, summary, moveList } = styles;

export function Tablebase({
  game,
  picture,
}: {
  game: number;
  picture: GamePicture;
}) {
  const [state, setState] = useState<
    TablebaseAnswer | { type: "Idle" | "Pending" }
  >({ type: "Idle" });
  const side = picture.turn === "black" ? "Black" : "White";

  async function lookup() {
    setState({ type: "Pending" });
    try {
      const { answer } = await ask({
        type: "LookupTablebase",
        game,
        revision: picture.revision,
      });
      setState(answer);
    } catch {
      setState({
        type: "Refused",
        reason: "Lichess tablebase is unavailable. Try again.",
      });
    }
  }

  return (
    <section aria-label="Lichess endgame tablebase" className={panel}>
      <div className={controls}>
        <button
          type="button"
          disabled={
            picture.tablebaseUnavailableReason !== null ||
            state.type === "Pending"
          }
          aria-describedby="tablebase-disclosure"
          onClick={lookup}
        >
          Ask Lichess
        </button>
        <span id="tablebase-disclosure">
          {picture.tablebaseUnavailableReason ??
            "Sends this position to the Lichess endgame tablebase."}
        </span>
      </div>
      {state.type === "Refused" && (
        <p role="alert" className={refusal}>
          {state.reason}
        </p>
      )}
      {state.type === "Evaluated" && (
        <>
          <p role="status" className={summary}>
            {side} to move: {state.outcome}
          </p>
          <div className={moveList}>
            <table>
              <thead>
                <tr>
                  <th scope="col">Move</th>
                  <th scope="col">Outcome for {side}</th>
                </tr>
              </thead>
              <tbody>
                {state.moves.map(({ san, outcome }) => (
                  <tr key={san}>
                    <td>{san}</td>
                    <td>{outcome}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  );
}
