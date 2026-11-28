import type { Component } from "../component";
import { files } from "../FilesPage";
import { user } from "../UserPage";
import { databases } from "./databases";
import { engines } from "./engines";
import { events } from "./events";

/** The one list of components. Its order is the order of the icon bar and of Home. */
export const components = [
  files,
  databases,
  events,
  engines,
  user,
] as const satisfies readonly Component[];

/** A page of the frame, or the page of a component. */
export type PageName =
  | "Home"
  | "Settings"
  | (typeof components)[number]["name"];
