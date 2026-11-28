import {
  ChessKnight,
  ChessPawn,
  ChevronDown,
  Monitor,
  User,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { Component } from "./component";
import { ask } from "./ipc";
import { LichessDownload } from "./LichessDownload";
import styles from "./UserPage.module.css";

const {
  content,
  profile,
  profileIcon,
  title,
  secondary,
  accounts,
  list,
  provider,
  row,
  piece,
  name: providerName,
  status,
  chevron,
  description: providerDescription,
  nameForm,
  actions,
  refusal,
} = styles;

const providers = [
  {
    name: "Lichess",
    icon: ChessKnight,
    description:
      "Download public games for your saved username. Dates are in UTC. Overlapping downloads can add the same game twice.",
  },
  {
    name: "Chess.com",
    icon: ChessPawn,
    description:
      "You can already download your games as PGN from Chess.com and open them in Files.",
  },
];

// Returning to User while a save finishes must read the name after that write.
let lastWrite: Promise<void> = Promise.resolve();

export function UserPage() {
  const [savedName, setSavedName] = useState<string | null>();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [writing, setWriting] = useState(false);

  useEffect(() => {
    let active = true;
    lastWrite
      .then(() => ask({ type: "GetLichessName" }))
      .then(
        ({ name }) => {
          if (active) {
            setSavedName(name);
            setDraft(name ?? "");
          }
        },
        (reason: unknown) => {
          if (active) setError(String(reason));
        },
      );
    return () => {
      active = false;
    };
  }, []);

  function changeName(name: string | null) {
    setWriting(true);
    setError("");
    const result =
      name === null
        ? ask({ type: "RemoveLichessName" }).then(() => null)
        : ask({ type: "SaveLichessName", name }).then((answer) => answer.name);
    lastWrite = result
      .then(
        (saved) => {
          setSavedName(saved);
          setDraft(saved ?? "");
        },
        (reason: unknown) => setError(String(reason)),
      )
      .finally(() => setWriting(false));
  }

  const errorMessage = error && (
    <span role="alert" className={refusal}>
      {error}
    </span>
  );

  return (
    <div className={content}>
      <div className={profile}>
        <span className={profileIcon}>
          <Monitor aria-hidden="true" />
        </span>
        <div>
          <h2 className={title}>Local profile</h2>
          <p className={secondary}>On this device</p>
        </div>
      </div>
      <section aria-labelledby="chess-accounts" className={accounts}>
        <h2 className={title} id="chess-accounts">
          Chess accounts
        </h2>
        <p className={secondary}>Account linking is not available yet.</p>
        {savedName === undefined && errorMessage}
        <div className={list}>
          {providers.map(({ name, icon: Icon, description }) => (
            <details key={name} className={provider}>
              <summary className={row}>
                <span className={piece}>
                  <Icon aria-hidden="true" />
                </span>
                <span className={providerName}>{name}</span>
                <span className={status}>
                  {name === "Lichess" && savedName != null
                    ? savedName
                    : "Not connected"}
                </span>
                <ChevronDown aria-hidden="true" className={chevron} />
              </summary>
              <p className={providerDescription}>{description}</p>
              {name === "Lichess" && (
                <>
                  <form
                    className={nameForm}
                    onSubmit={(event) => {
                      event.preventDefault();
                      changeName(draft);
                    }}
                  >
                    <label htmlFor="lichess-username">Lichess username</label>
                    <input
                      id="lichess-username"
                      autoComplete="off"
                      spellCheck={false}
                      value={draft}
                      disabled={savedName === undefined || writing}
                      onChange={(event) => {
                        setDraft(event.target.value);
                        setError("");
                      }}
                    />
                    <div className={actions}>
                      <button
                        type="submit"
                        disabled={savedName === undefined || writing}
                      >
                        Save
                      </button>
                      {savedName != null && (
                        <button
                          type="button"
                          disabled={writing}
                          onClick={() => changeName(null)}
                        >
                          Remove name
                        </button>
                      )}
                    </div>
                    {savedName !== undefined && errorMessage}
                  </form>
                  <LichessDownload savedName={savedName} writing={writing} />
                </>
              )}
            </details>
          ))}
        </div>
      </section>
    </div>
  );
}

export const user = {
  name: "User",
  page: { icon: User, view: () => <UserPage /> },
} as const satisfies Component;
