import { useLayoutEffect, useRef, useState } from "react";
import { type BoardPlace, type Change, sameBoardPlace } from "./board/tabs";
import styles from "./EngineAnalysis.module.css";
import type { Response } from "./generated/Response";
import { ask } from "./ipc";

type Snapshot = Extract<Response, { type: "Analysis" }>["snapshot"];
const { panel } = styles;

let lastGeneration = 0;

export function followAnalysis(
  game: number,
  origin: BoardPlace,
  show: (snapshot: Snapshot) => void,
) {
  const generation = ++lastGeneration;
  let active = true;
  let pending = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function stop() {
    if (!active) return;
    active = false;
    clearTimeout(timer);
    void ask({
      type: "Analysis",
      game,
      action: "Stop",
      generation,
    }).catch(() => {
      // Closing the board can dispose of the game before this request arrives.
    });
  }

  function waiting() {
    show({
      game,
      generation,
      revision: origin.revision,
      place: origin.place,
      state: { type: "Searching", lines: [] },
    });
  }

  async function read(type: "Start" | "Status"): Promise<void> {
    clearTimeout(timer);
    pending = true;
    const askedAt = origin;
    try {
      const { snapshot } = await ask({
        type: "Analysis",
        game,
        action: type,
        generation,
      });
      if (!active) return;
      if (
        snapshot.game === game &&
        snapshot.generation === generation &&
        sameBoardPlace(snapshot, origin)
      ) {
        show(snapshot);
        if (snapshot.state.type !== "Searching") {
          stop();
          return;
        }
      }
    } catch (reason) {
      if (active) {
        const message =
          reason instanceof Error ? reason.message : String(reason);
        show({
          game,
          revision: origin.revision,
          place: origin.place,
          generation,
          state: { type: "Failed", message },
        });
        stop();
      }
      return;
    } finally {
      pending = false;
    }
    if (origin !== askedAt) {
      void read("Status");
    } else {
      timer = setTimeout(() => void read("Status"), 250);
    }
  }

  waiting();
  void read("Start");
  return {
    follow(next: BoardPlace) {
      if (!active || sameBoardPlace(origin, next)) return;
      origin = next;
      waiting();
      if (!pending) void read("Status");
    },
    stop,
  };
}

export function EngineAnalysis({
  game,
  origin,
  change,
  hidden,
}: {
  game: number;
  origin: BoardPlace;
  change: (change: Change) => void;
  hidden: boolean;
}) {
  const search = useRef<ReturnType<typeof followAnalysis> | undefined>(
    undefined,
  );
  const [snapshot, setSnapshot] = useState<Snapshot>();
  useLayoutEffect(() => () => search.current?.stop(), []);
  useLayoutEffect(() => search.current?.follow(origin), [origin]);
  const running = snapshot?.state.type === "Searching";
  const displayed =
    snapshot && sameBoardPlace(snapshot, origin) ? snapshot : undefined;
  const state = displayed?.state;

  async function addVariation(row: Snapshot, moves: string[]) {
    const { game, revision, generation, place } = row;
    const { answer } = await ask({
      type: "AddEngineVariation",
      game,
      revision,
      generation,
      place,
      moves,
    });
    if (answer.type === "Added") {
      change({ type: "drawPicture", game, picture: answer.picture, from: row });
    }
  }

  return (
    <section className={panel} aria-label="Engine analysis" hidden={hidden}>
      <header>
        <h2>Engine analysis</h2>
        <button
          type="button"
          onClick={() => {
            if (running) {
              search.current?.stop();
              setSnapshot(undefined);
            } else {
              search.current = followAnalysis(game, origin, setSnapshot);
            }
          }}
        >
          {running ? "Stop engine" : "Start engine"}
        </button>
      </header>
      {state?.type === "Failed" && <p role="alert">{state.message}</p>}
      {displayed &&
        state?.type === "Searching" &&
        (state.lines.length === 0 ? (
          <p>Analysing…</p>
        ) : (
          <ol>
            {state.lines.map((line) => (
              <li key={line.notation}>
                <strong>{line.evaluation}</strong>
                <span>{line.notation}</span>
                <button
                  type="button"
                  onClick={() => void addVariation(displayed, line.moves)}
                >
                  Add variation
                </button>
              </li>
            ))}
          </ol>
        ))}
    </section>
  );
}
