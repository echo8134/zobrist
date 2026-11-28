import { Suspense, use, useActionState, useState } from "react";
import { BoardSettingsForm } from "../board/BoardSettings";
import type { Change } from "../board/tabs";
import type { Component, PageApp } from "../component";
import type { PageName } from "../components/list";
import formStyles from "../controls/CollectionPage.module.css";
import { ask } from "../ipc";
import styles from "./PageArea.module.css";

const { area, heading, versionLine, gameLink } = styles;
const { toolbar, button, line, refusal } = formStyles;

const version = ask({ type: "Version" });

function VersionLine() {
  return <p className={versionLine}>zobrist {use(version).version}</p>;
}

/** A component of the list with the handle that the frame made for it. */
export type Listed = { component: Component; app: PageApp };

type Props = {
  page: PageName;
  change: (change: Change) => void;
  listed: readonly Listed[];
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

export function PageArea({ page, change, listed }: Props) {
  const shown = listed.find(({ component }) => component.name === page);
  return (
    <main className={area}>
      <h1 className={heading}>{page}</h1>
      {page === "Home" ? (
        <>
          <GameLinkForm change={change} />
          {listed.map(
            ({ component: { name, home: Home }, app }) =>
              Home && <Home key={name} app={app} />,
          )}
        </>
      ) : page === "Settings" ? (
        <Suspense>
          <VersionLine />
          <BoardSettingsForm />
        </Suspense>
      ) : (
        shown && <shown.component.page.view app={shown.app} />
      )}
    </main>
  );
}
