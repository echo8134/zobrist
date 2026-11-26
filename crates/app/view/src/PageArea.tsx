import { type ReactNode, Suspense, use } from "react";
import { ask } from "./ipc";
import styles from "./PageArea.module.css";

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

const contents: Record<Page, ReactNode> = {
  Home: null,
  Files: null,
  Databases: null,
  Events: null,
  Engines: null,
  User: null,
  Settings: (
    <Suspense>
      <VersionLine />
    </Suspense>
  ),
};

export function PageArea({ page }: { page: Page }) {
  return (
    <main className={area}>
      <h1 className={heading}>{page}</h1>
      {contents[page]}
    </main>
  );
}
