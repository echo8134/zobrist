import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export function nativeKey(
  action: "tab" | "enter" | "copy-selection" | "paste",
) {
  execFileSync("python3", [
    fileURLToPath(new URL("./native-key.py", import.meta.url)),
    action,
  ]);
}
