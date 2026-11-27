import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { openEngines } from "./engine-session";
import { pick } from "./file-picker";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));

async function checkRows(
  session: WebdriverIO.Browser,
  work: string,
  names: string[],
) {
  const rows = session.$$("main li");
  await expect(rows).toBeElementsArrayOfSize(names.length);
  for (const [index, name] of names.entries()) {
    const row = session.$(`main li:nth-child(${index + 1})`);
    await expect(row.$("strong")).toHaveText(name);
    await expect(row.$("div span")).toHaveText(join(work, name));
    if (index === 0) {
      await expect(row.$("span=Default")).toBeDisplayed();
    } else {
      await expect(row.$("span=Default")).not.toExist();
      await expect(row.$("button=Make default")).toBeDisplayed();
    }
  }
}

describe("the page Engines", () => {
  it("remembers local engine paths and the player's default after restarting", async function () {
    this.timeout(60_000);
    const work = mkdtempSync(join(tmpdir(), "zobrist-engines-"));
    const first = join(work, "sample-a");
    const second = join(work, "sample-b");
    const empty = "No local engines added.";
    try {
      for (const path of [first, second]) {
        writeFileSync(path, '#!/bin/sh\ntouch "$0.launched"\n', {
          mode: 0o700,
        });
      }
      await openEngines(work, async (session) => {
        await expect(session.$("main p")).toHaveText(empty);
        await pick("Engine", null, session);
        await expect(session.$("main p")).toHaveText(empty);
        for (const path of [first, second, first]) {
          await pick("Engine", path, session);
          await expect(session.$("main").$("aria/Add engine…")).toBeEnabled();
        }
        await checkRows(session, work, ["sample-a", "sample-b"]);
        await session.$("main").$("button=Make default").click();
        await checkRows(session, work, ["sample-b", "sample-a"]);
        mkdirSync(join(target, "e2e"), { recursive: true });
        const picture = await session.saveScreenshot(
          join(target, "e2e", "engines.png"),
        );
        expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
          1280, 800,
        ]);
      });
      await openEngines(work, async (session) => {
        await checkRows(session, work, ["sample-b", "sample-a"]);
        await pick("Engine", join(work, "missing"), session);
        await expect(session.$('main [role="alert"]')).toHaveText(
          "Engine file is unavailable.",
        );
        await checkRows(session, work, ["sample-b", "sample-a"]);
        await session.$("main li").$("button=Remove").click();
        await checkRows(session, work, ["sample-a"]);
        expect(existsSync(second)).toBe(true);
        await session.$("main li").$("button=Remove").click();
        await expect(session.$("main p")).toHaveText(empty);
        expect(existsSync(first)).toBe(true);
      });
      expect(
        [first, second].map((path) => existsSync(`${path}.launched`)),
      ).toEqual([false, false]);
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
});
