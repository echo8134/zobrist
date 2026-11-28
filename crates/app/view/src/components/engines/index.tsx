import { Cpu } from "lucide-react";
import type { Component } from "../../component";
import { EnginesPage } from "./EnginesPage";

export const engines = {
  name: "Engines",
  page: { icon: Cpu, view: () => <EnginesPage /> },
} as const satisfies Component;
