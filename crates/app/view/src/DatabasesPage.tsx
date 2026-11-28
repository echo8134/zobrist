import { Database } from "lucide-react";
import { useEffect, useSyncExternalStore } from "react";
import { DatabaseGames } from "./DatabaseGames";
import styles from "./FilesPage.module.css";
import type { DatabaseEntry } from "./generated/DatabaseEntry";
import { ask } from "./ipc";
import type { Change } from "./tabs";

const {
  toolbar,
  button,
  workspace,
  collection,
  caption,
  line,
  tree,
  entryButton,
  details,
  titleRow,
  title,
  count,
  pathText,
  refusal,
} = styles;

type Selection = { path: string } & (
  | { kind: "Checking" }
  | { kind: "Count"; games: number }
  | {
      kind: "Imported";
      games: number;
      problems: number;
      firstProblem: string | null;
    }
  | { kind: "Failed"; message: string }
);

type Kept = {
  entries: DatabaseEntry[] | null;
  selected: Selection | null;
  error: { source: "Import" | "Page"; message: string } | null;
  activity: "Ready" | "Busy" | "Importing";
};

let kept: Kept = {
  entries: null,
  selected: null,
  error: null,
  activity: "Ready",
};
const listeners = new Set<() => void>();

function keep(change: Partial<Kept>) {
  kept = { ...kept, ...change };
  for (const notify of listeners) notify();
}

function subscribe(notify: () => void) {
  listeners.add(notify);
  return () => {
    listeners.delete(notify);
  };
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function run(action: () => Promise<void>) {
  if (kept.activity !== "Ready") return;
  // Rereading the list on return must preserve the saved path of a failed import.
  const importError = kept.error?.source === "Import" ? kept.error : null;
  keep({ activity: "Busy", error: importError });
  void action()
    .catch((cause: unknown) => {
      if (kept.error?.source !== "Import") {
        keep({ error: { source: "Page", message: errorText(cause) } });
      }
    })
    .finally(() => keep({ activity: "Ready" }));
}

function counted(
  path: string,
  games: number,
  previous: Selection | null,
): Selection {
  return previous?.kind === "Imported" && previous.path === path
    ? { ...previous, games }
    : { kind: "Count", path, games };
}

export function DatabasesPage({
  change,
}: {
  change: (change: Change) => void;
}) {
  const { entries, selected, error, activity } = useSyncExternalStore(
    subscribe,
    () => kept,
  );
  const busy = activity !== "Ready";

  useEffect(() => {
    run(async () =>
      keep({ entries: (await ask({ type: "ListDatabases" })).entries }),
    );
  }, []);

  function add() {
    run(async () => {
      const { path } = await ask({ type: "PickDatabasePath" });
      if (path === null) return;
      keep({ error: null });
      const answer = await ask({ type: "AddDatabase", path });
      keep({
        entries: answer.entries,
        selected: counted(path, answer.games, kept.selected),
      });
    });
  }

  function importPgn() {
    run(async () => {
      const { path: source } = await ask({
        type: "PickPgnPath",
        choice: "File",
        folder: null,
      });
      if (source === null) return;
      const { path: destination } = await ask({ type: "PickNewDatabasePath" });
      if (destination === null) return;
      const previous = kept.selected;
      keep({
        activity: "Importing",
        error: null,
        selected:
          previous?.kind === "Imported"
            ? { kind: "Count", path: previous.path, games: previous.games }
            : previous,
      });
      try {
        const { entries, path, games, problems, firstProblem } = await ask({
          type: "ImportDatabase",
          source,
          destination,
        });
        keep({
          entries,
          selected: { kind: "Imported", path, games, problems, firstProblem },
        });
      } catch (cause) {
        keep({ error: { source: "Import", message: errorText(cause) } });
      }
    });
  }

  function inspect(path: string) {
    run(async () => {
      const previous = kept.selected;
      keep({ selected: { kind: "Checking", path }, error: null });
      try {
        const { games } = await ask({ type: "InspectDatabase", path });
        keep({ selected: counted(path, games, previous) });
      } catch (cause) {
        keep({ selected: { kind: "Failed", path, message: errorText(cause) } });
      }
    });
  }

  function open(path: string, number: number) {
    run(async () => {
      const { answer } = await ask({ type: "OpenDatabaseGame", path, number });
      if (answer.type === "Refusal") {
        const reason =
          answer.reason === "noSuchGame"
            ? `The collection has no game numbered ${number}.`
            : `Game ${number} could not be opened from ${path}.`;
        throw new Error(answer.system ? `${reason} ${answer.system}` : reason);
      }
      change({ type: "openTab", game: answer.game, picture: answer.picture });
    });
  }

  function remove(path: string) {
    run(async () => {
      const { entries } = await ask({ type: "RemoveDatabase", path });
      keep({ entries, selected: null, error: null });
    });
  }

  const entry = entries?.find(({ path }) => path === selected?.path);
  return (
    <>
      <div className={toolbar}>
        <button className={button} type="button" disabled={busy} onClick={add}>
          Add collection…
        </button>
        <button
          className={button}
          type="button"
          disabled={busy}
          onClick={importPgn}
        >
          {activity === "Importing" ? "Importing PGN…" : "Import PGN…"}
        </button>
      </div>
      <div className={workspace}>
        <section className={collection} aria-label="This device">
          <h2 className={caption}>This device</h2>
          {entries?.length === 0 && <p className={line}>No collections yet.</p>}
          <ul className={tree}>
            {entries?.map(({ path, name }) => (
              <li key={path}>
                <button
                  type="button"
                  className={entryButton}
                  aria-current={path === selected?.path}
                  title={path}
                  disabled={busy}
                  onClick={() => inspect(path)}
                >
                  <Database aria-hidden="true" />
                  <span>{name}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
        <section className={details} aria-label="Selected collection">
          {error && (
            <p className={refusal} role="status">
              {error.message}
            </p>
          )}
          {entry && selected && (
            <>
              <div className={titleRow}>
                <h2 className={title}>{entry.name}</h2>
                {"games" in selected && (
                  <span className={count}>
                    {selected.games} {selected.games === 1 ? "game" : "games"}
                  </span>
                )}
              </div>
              <p className={pathText}>{entry.path}</p>
              {selected.kind === "Imported" && (
                <div role="status">
                  <p className={line}>
                    {selected.problems}{" "}
                    {selected.problems === 1 ? "problem" : "problems"}
                  </p>
                  {selected.firstProblem !== null && (
                    <p className={line}>{selected.firstProblem}</p>
                  )}
                </div>
              )}
              {selected.kind === "Checking" && (
                <p className={line} role="status">
                  Checking collection…
                </p>
              )}
              {selected.kind === "Failed" && (
                <p className={refusal} role="status">
                  {selected.message}
                </p>
              )}
              <div className={toolbar}>
                <button
                  className={button}
                  type="button"
                  disabled={busy}
                  onClick={() => remove(entry.path)}
                >
                  Remove from list
                </button>
              </div>
              {"games" in selected && (
                <DatabaseGames
                  key={entry.path}
                  path={entry.path}
                  open={(number) => open(entry.path, number)}
                />
              )}
            </>
          )}
        </section>
      </div>
    </>
  );
}
