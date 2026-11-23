import { StrictMode, Suspense, use } from "react";
import { createRoot } from "react-dom/client";
import { ask } from "./ipc";

const version = ask({ type: "Version" });

function VersionLine() {
  return <p>zobrist {use(version).version}</p>;
}

const root = document.getElementById("root");
if (root === null) {
  throw new Error("index.html has no element with the id root");
}
createRoot(root).render(
  <StrictMode>
    <Suspense>
      <VersionLine />
    </Suspense>
  </StrictMode>,
);
