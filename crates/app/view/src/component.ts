import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import type { ChangedGameFile } from "./generated/ChangedGameFile";
import type { GamePicture } from "./generated/GamePicture";

/** One topic of the app. It offers faces. The frame decides where a face stands and whether it shows. */
export type Component = {
  /** Unique. The label in the icon bar and the heading of the page. */
  name: string;
  page: Page;
  /** A section on Home, in the order of the list. */
  home?: (props: { app: PageApp }) => ReactNode;
  /** What the component does when a fact is told. It runs also while no face of the component is shown. */
  hears?: {
    [Notice in keyof Notices]?: (detail: Notices[Notice], app: PageApp) => void;
  };
  /** The PGN file that takes a game without a file. At most one component answers. */
  saveTarget?: () => string | undefined;
};

type Page = { icon: LucideIcon; view: (props: { app: PageApp }) => ReactNode };

export type App = {
  /** Puts a game that Rust opened on a board and shows that board. */
  openOnBoard(opened: { game: number; picture: GamePicture }): void;
  /** Tells a fact to the frame and to every component that hears it. */
  tell<Notice extends keyof Notices>(
    notice: Notice,
    detail: Notices[Notice],
  ): void;
};
/** Only a component with a page can show a page, and only its own. */
export type PageApp = App & { showPage(): void };

/** The facts that cross components. A component never calls another one. */
export type Notices = {
  /** Games were written into this PGN file. */
  gamesWritten: { path: string };
  /** A game of a file moved in its file or left it. */
  gameFileChanged: ChangedGameFile;
  /** The player dropped paths on the window. */
  pathsDropped: { paths: string[] };
};
