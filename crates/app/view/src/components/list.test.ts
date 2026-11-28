import { expect, test } from "vitest";
import { components } from "./list";

const folders = new Set(
  Object.keys(import.meta.glob("./*/**")).map((path) => path.split("/")[1]),
);
const entryFiles = import.meta.glob<Record<string, unknown>>("./*/index.tsx", {
  eager: true,
});
const listed: readonly unknown[] = components;

test("every component folder has an entry in the list", () => {
  expect(folders.size).toBeGreaterThan(0);
  for (const folder of folders) {
    const exported = Object.values(entryFiles[`./${folder}/index.tsx`] ?? {});
    expect(
      exported.some((entry) => listed.includes(entry)),
      `components/${folder} has no entry in src/components/list.ts.`,
    ).toBe(true);
  }
});
