import { Database } from "lucide-react";
import type { Component } from "../../component";
import { DatabasesPage } from "./DatabasesPage";

export const databases = {
  name: "Databases",
  page: {
    icon: Database,
    view: ({ app }) => <DatabasesPage onOpened={app.openOnBoard} />,
  },
} as const satisfies Component;
