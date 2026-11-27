import "@fontsource-variable/inter";
import "./design.css";
import { StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { BoardSettingsProvider } from "./BoardSettings";
import { Frame } from "./Frame";

const root = document.getElementById("root");
if (root === null) {
  throw new Error("index.html has no element with the id root");
}
createRoot(root).render(
  <StrictMode>
    <Suspense>
      <BoardSettingsProvider>
        <Frame />
      </BoardSettingsProvider>
    </Suspense>
  </StrictMode>,
);
