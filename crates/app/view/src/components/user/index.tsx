import { User } from "lucide-react";
import type { Component } from "../../component";
import { UserPage } from "./UserPage";

export const user = {
  name: "User",
  page: {
    icon: User,
    view: ({ app }) => (
      <UserPage onGamesWritten={(path) => app.tell("gamesWritten", { path })} />
    ),
  },
} as const satisfies Component;
