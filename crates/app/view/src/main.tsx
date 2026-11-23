import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

const root = document.getElementById("root");
if (root === null) {
  throw new Error("index.html has no element with the id root");
}
createRoot(root).render(<StrictMode />);
