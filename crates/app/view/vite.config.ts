import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const fontLicense = fileURLToPath(
  import.meta.resolve("@fontsource-variable/inter/LICENSE"),
);

export default defineConfig({
  plugins: [
    {
      name: "font-license",
      // The license of the font requires that its text goes with the font files.
      generateBundle() {
        this.emitFile({
          type: "asset",
          fileName: "assets/inter-license.txt",
          source: readFileSync(fontLicense),
        });
      },
    },
  ],
});
