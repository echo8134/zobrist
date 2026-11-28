import { Database } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import styles from "./FilesPage.module.css";
import type { DatabaseEntry } from "./generated/DatabaseEntry";
import { ask } from "./ipc";

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

// Returning to the page must load the list after a pending add or removal.
let pending = Promise.resolve();

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function DatabasesPage() {
  const [entries, setEntries] = useState<DatabaseEntry[] | null>(null);
  const [selected, setSelected] = useState<Selection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activity, setActivity] = useState<"Ready" | "Busy" | "Importing">(
    "Busy",
  );
  const busy = activity !== "Ready";

  const run = useCallback((action: () => Promise<void>) => {
    setActivity("Busy");
    setError(null);
    pending = pending
      .then(action)
      .catch((cause: unknown) => setError(errorText(cause)))
      .finally(() => setActivity("Ready"));
  }, []);

  useEffect(() => {
    run(async () => setEntries((await ask({ type: "ListDatabases" })).entries));
  }, [run]);

  function add() {
    run(async () => {
      const { path } = await ask({ type: "PickDatabasePath" });
      if (path === null) return;
      const answer = await ask({ type: "AddDatabase", path });
      setEntries(answer.entries);
      setSelected({ kind: "Count", path, games: answer.games });
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
      setActivity("Importing");
      const { entries, path, games, problems, firstProblem } = await ask({
        type: "ImportDatabase",
        source,
        destination,
      });
      setEntries(entries);
      setSelected({ kind: "Imported", path, games, problems, firstProblem });
    });
  }

  function inspect(path: string) {
    setSelected({ kind: "Checking", path });
    run(async () => {
      try {
        const { games } = await ask({ type: "InspectDatabase", path });
        setSelected({ kind: "Count", path, games });
      } catch (cause) {
        setSelected({ kind: "Failed", path, message: errorText(cause) });
      }
    });
  }

  function remove(path: string) {
    run(async () => {
      setEntries((await ask({ type: "RemoveDatabase", path })).entries);
      setSelected(null);
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
            </>
          )}
          {error && (
            <p className={refusal} role="status">
              {error}
            </p>
          )}
        </section>
      </div>
    </>
  );
}
