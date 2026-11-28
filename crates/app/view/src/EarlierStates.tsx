import {
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import styles from "./controls/CollectionPage.module.css";
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
  confirmButton,
} = styles;

export function FileConfirmation({
  question,
  action,
  children,
  onCancel,
  onConfirm,
}: {
  question: string;
  action: "Restore" | "Delete";
  children?: ReactNode;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const heading = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  useLayoutEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      className={confirmation}
      aria-labelledby={heading}
      ref={dialog}
      onClose={({ currentTarget }) => {
        if (currentTarget.returnValue === "confirm") onConfirm();
        else onCancel();
      }}
    >
      <h2 id={heading} className={title}>
        {question}
      </h2>
      {children}
      <form method="dialog" className={confirmationActions}>
        <button type="submit" value="cancel" className={button}>
          Cancel
        </button>
        <button
          type="submit"
          value="confirm"
          className={`${button} ${confirmButton}`}
        >
          {action}
        </button>
      </form>
    </dialog>
  );
}

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
        <FileConfirmation
          question={`Restore State ${action.number}?`}
          action="Restore"
          onCancel={() => setAction(null)}
          onConfirm={() => void restore(action.number, action.stamp)}
        >
          <p className={pathText}>{file.path}</p>
          <p>The current contents will be kept as an earlier state.</p>
        </FileConfirmation>
      )}
    </section>
  );
}
