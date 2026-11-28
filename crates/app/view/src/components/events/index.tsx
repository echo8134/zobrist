import { Calendar } from "lucide-react";
import type { Component } from "../../component";

export const events = {
  name: "Events",
  page: { icon: Calendar, view: () => null },
} as const satisfies Component;
