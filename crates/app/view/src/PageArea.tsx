import { type ReactNode, Suspense, use } from "react";
import { BoardSettingsForm } from "./BoardSettings";
import { FilesPage } from "./FilesPage";
import { ask } from "./ipc";
import styles from "./PageArea.module.css";
import { UserPage } from "./UserPage";

const { area, heading, versionLine } = styles;

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

const contents: Record<Exclude<Page, "Files">, ReactNode> = {
  Home: null,
  Databases: null,
  Events: null,
  Engines: null,
  User: <UserPage />,
  Settings: (
    <Suspense>
      <VersionLine />
      <BoardSettingsForm />
    </Suspense>
  ),
};

type Props = {
  page: Page;
  onOpenGame: (path: string, number: number) => Promise<boolean>;
};

export function PageArea({ page, onOpenGame }: Props) {
  return (
    <main className={area}>
      <h1 className={heading}>{page}</h1>
      {page === "Files" ? (
        <FilesPage onOpenGame={onOpenGame} />
      ) : (
        contents[page]
      )}
    </main>
  );
}
