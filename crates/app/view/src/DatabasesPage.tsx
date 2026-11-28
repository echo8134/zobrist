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
  const [busy, setBusy] = useState(true);

  const run = useCallback((action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    pending = pending
      .then(action)
      .catch((cause: unknown) => setError(errorText(cause)))
      .finally(() => setBusy(false));
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
                {selected.kind === "Count" && (
                  <span className={count}>
                    {selected.games} {selected.games === 1 ? "game" : "games"}
                  </span>
                )}
              </div>
              <p className={pathText}>{entry.path}</p>
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
