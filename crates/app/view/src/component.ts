import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import type { GamePicture } from "./generated/GamePicture";

/** One topic of the app. It offers faces. The frame decides where a face stands and whether it shows. */
export type Component = {
  /** Unique. The label in the icon bar and the heading of the page. */
  name: string;
  page: Page;
};

type Page = { icon: LucideIcon; view: (props: { app: App }) => ReactNode };

export type App = {
  /** Puts a game that Rust opened on a board and shows that board. */
  openOnBoard(opened: { game: number; picture: GamePicture }): void;
};
