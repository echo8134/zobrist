import { Folder } from "lucide-react";
import type { Component } from "../../component";
import {
  FilesPage,
  fileOfThePage,
  openGame,
  openListed,
  openPgnPath,
  readFileAgain,
} from "./FilesPage";
import { RecentFiles } from "./RecentFiles";

export const files = {
  name: "Files",
  page: {
    icon: Folder,
    view: ({ app }) => (
      <FilesPage
        onOpenGame={(path, number) => openGame(path, number, app.openOnBoard)}
        onGameFileChanged={(changed) => app.tell("gameFileChanged", changed)}
      />
    ),
  },
  home: ({ app }) => (
    <RecentFiles
      onOpen={(entry) => {
        app.showPage();
        void openListed(entry);
      }}
    />
  ),
  hears: {
    gamesWritten: () => void readFileAgain(),
    pathsDropped: (dropped, app) => {
      app.showPage();
      void openPgnPath(dropped);
    },
  },
  saveTarget: fileOfThePage,
} as const satisfies Component;
