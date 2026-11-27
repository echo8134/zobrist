import { type ReactNode, Suspense, use, useActionState, useState } from "react";
import { FilesPage } from "./FilesPage";
import formStyles from "./FilesPage.module.css";
import { ask } from "./ipc";
import styles from "./PageArea.module.css";
import { type Change, openGame } from "./tabs";
import { UserPage } from "./UserPage";

const { area, heading, versionLine, gameLink } = styles;
const { toolbar, button, line, refusal } = formStyles;

/** The pages in the order of their entries in the icon bar. */
export const upperPages = [
  "Home",
  "Files",
  "Databases",
  "Events",
  "Engines",
] as const;
/** The pages whose entries stand at the bottom of the icon bar. */
export const lowerPages = ["User", "Settings"] as const;

export type Page = (typeof upperPages)[number] | (typeof lowerPages)[number];

const version = ask({ type: "Version" });

function VersionLine() {
  return <p className={versionLine}>zobrist {use(version).version}</p>;
}

type Props = {
  page: Page;
  change: (change: Change) => void;
};

function GameLinkForm({ change }: Pick<Props, "change">) {
  const [link, setLink] = useState("");
  const [reason, submit, pending] = useActionState(async () => {
    const { answer } = await ask({ type: "OpenGameLink", link });
    if (answer.type === "Refused") {
      return answer.reason;
    }
    change({ type: "openTab", game: answer.game, picture: answer.picture });
    return undefined;
  }, undefined);

  return (
    <>
      <form className={toolbar} action={submit}>
        <input
          className={gameLink}
          type="text"
          inputMode="url"
          aria-label="Lichess game link"
          placeholder="Lichess game link"
          autoComplete="off"
          spellCheck={false}
          value={link}
          onChange={(event) => setLink(event.target.value)}
          disabled={pending}
        />
        <button type="submit" className={button} disabled={pending}>
          Open game link
        </button>
      </form>
      {(pending || reason !== undefined) && (
        <p className={pending ? line : `${line} ${refusal}`} role="status">
          {pending ? "Opening game…" : reason}
        </p>
      )}
    </>
  );
}

const contents: Record<Exclude<Page, "Files" | "Home">, ReactNode> = {
  Databases: null,
  Events: null,
  Engines: null,
  User: <UserPage />,
  Settings: (
    <Suspense>
      <VersionLine />
    </Suspense>
  ),
};

export function PageArea({ page, change }: Props) {
  return (
    <main className={area}>
      <h1 className={heading}>{page}</h1>
      {page === "Home" ? (
        <GameLinkForm change={change} />
      ) : page === "Files" ? (
        <FilesPage
          onOpenGame={(path, number) => openGame(path, number, change)}
        />
      ) : (
        contents[page]
      )}
    </main>
  );
}
