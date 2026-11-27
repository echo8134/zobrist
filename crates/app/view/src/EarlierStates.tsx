import { useEffect, useState } from "react";
import styles from "./FilesPage.module.css";
import type { EarlierState } from "./generated/EarlierState";
import { ask, errorText } from "./ipc";

const {
  history,
  caption,
  line,
  tree,
  stateRow,
  refusal,
  confirmation,
  title,
  pathText,
  confirmationActions,
  button,
  restore: restoreButton,
} = styles;

type Action =
  | { kind: "Busy" }
  | { kind: "Confirm"; number: number; stamp: string }
  | { kind: "Error"; message: string };

type Props = {
  file: { path: string };
  onRestored: (path: string) => Promise<void>;
};

export function EarlierStates({ file, onRestored }: Props) {
  const [listing, setListing] = useState<
    { states: EarlierState[] } | { error: string } | null
  >(null);
  const [action, setAction] = useState<Action | null>(null);
  useEffect(() => {
    let current = true;
    void ask({ type: "ListPgnStates", path: file.path }).then(
      ({ states }) => {
        if (current) setListing({ states });
      },
      (error: unknown) => {
        if (current) setListing({ error: errorText(error) });
      },
    );
    return () => {
      current = false;
    };
  }, [file]);

  const choose = async (number: number) => {
    setAction({ kind: "Busy" });
    try {
      const { stamp } = await ask({
        type: "PreparePgnRestore",
        path: file.path,
        number,
      });
      setAction({ kind: "Confirm", number, stamp });
    } catch (error) {
      setAction({ kind: "Error", message: errorText(error) });
    }
  };

  const restore = async (number: number, stamp: string) => {
    setAction({ kind: "Busy" });
    try {
      await ask({ type: "RestorePgnState", path: file.path, number, stamp });
      await onRestored(file.path);
      setAction(null);
    } catch (error) {
      setAction({ kind: "Error", message: errorText(error) });
    }
  };

  const error =
    action?.kind === "Error"
      ? action.message
      : listing && "error" in listing
        ? listing.error
        : null;
  return (
    <section className={history} aria-label="Earlier states">
      <h3 className={caption}>Earlier states</h3>
      {listing === null && <p className={line}>Loading earlier states…</p>}
      {listing &&
        "states" in listing &&
        (listing.states.length === 0 ? (
          <p className={line}>No earlier states.</p>
        ) : (
          <ul className={tree}>
            {listing.states.map(({ number, details }) => (
              <li key={number}>
                <button
                  type="button"
                  className={stateRow}
                  aria-label={`State ${number}`}
                  disabled={action?.kind === "Busy"}
                  onClick={() => void choose(number)}
                >
                  <span>State {number}</span>
                  {details ? (
                    <>
                      <span>
                        {new Date(details.seconds * 1000)
                          .toISOString()
                          .replace("T", " ")
                          .replace(".000Z", " UTC")}
                      </span>
                      <span>
                        {details.bytes} {details.bytes === 1 ? "byte" : "bytes"}
                      </span>
                    </>
                  ) : (
                    <span>Unreadable state</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        ))}
      {error && (
        <p className={refusal} role="alert">
          {error}
        </p>
      )}
      {action?.kind === "Confirm" && (
        <dialog
          className={confirmation}
          aria-labelledby="restore-state-heading"
          ref={(element) => {
            if (element && !element.open) element.showModal();
          }}
          onCancel={() => setAction(null)}
        >
          <h2 id="restore-state-heading" className={title}>
            Restore State {action.number}?
          </h2>
          <p className={pathText}>{file.path}</p>
          <p>The current contents will be kept as an earlier state.</p>
          <div className={confirmationActions}>
            <button
              type="button"
              className={button}
              onClick={() => setAction(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              className={`${button} ${restoreButton}`}
              onClick={() => void restore(action.number, action.stamp)}
            >
              Restore
            </button>
          </div>
        </dialog>
      )}
    </section>
  );
}
