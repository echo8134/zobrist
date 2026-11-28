import { useCallback, useEffect, useState } from "react";
import type { DesktopRequest } from "../../generated/DesktopRequest";
import type { Response } from "../../generated/Response";
import { ask } from "../../ipc";
import styles from "./EnginesPage.module.css";

const { panel, engines, engine } = styles;

type Action = Extract<DesktopRequest, { type: "EngineInventory" }>["action"];
type Entries = Extract<Response, { type: "EngineInventory" }>["entries"];

// A page reopened during a picker or a write reads the inventory after it finishes.
let lastChange = Promise.resolve();

export function EnginesPage() {
  const [entries, setEntries] = useState<Entries>();
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");

  const send = useCallback(async (choice: Action | "Pick") => {
    setBusy(true);
    setError("");
    try {
      let action: Action;
      if (choice === "Pick") {
        const { path } = await ask({ type: "PickEnginePath" });
        if (path === null) return;
        action = { type: "Register", path };
      } else {
        action = choice;
      }
      const answer = await ask({ type: "EngineInventory", action });
      setEntries(answer.entries);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    let current = true;
    void lastChange.then(() => {
      if (current) void send({ type: "List" });
    });
    return () => {
      current = false;
    };
  }, [send]);

  function change(choice: Action | "Pick") {
    lastChange = send(choice);
  }

  return (
    <section className={panel} aria-label="Local engines">
      <button type="button" disabled={busy} onClick={() => change("Pick")}>
        Add engine…
      </button>
      {error && <p role="alert">{error}</p>}
      {entries?.length === 0 && <p>No local engines added.</p>}
      {entries && entries.length > 0 && (
        <ul className={engines}>
          {entries.map(({ name, path }, index) => (
            <li key={path}>
              <div className={engine} id={`engine-${index}`}>
                <strong>{name}</strong>
                <span>{path}</span>
              </div>
              {index === 0 ? (
                <span>Default</span>
              ) : (
                <button
                  type="button"
                  disabled={busy}
                  aria-describedby={`engine-${index}`}
                  onClick={() => change({ type: "SetDefault", path })}
                >
                  Make default
                </button>
              )}
              <button
                type="button"
                disabled={busy}
                aria-describedby={`engine-${index}`}
                onClick={() => change({ type: "Remove", path })}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
