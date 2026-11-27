import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { cleanupWdioSession, startWdioSession } from "@wdio/tauri-service";
import { pick } from "./file-picker";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));

async function freePort() {
  const socket = createServer();
  await new Promise<void>((ready, reject) => {
    socket.once("error", reject).listen(0, "127.0.0.1", ready);
  });
  const address = socket.address();
  await new Promise<void>((done) => socket.close(() => done()));
  if (!address || typeof address === "string") throw Error("Expected a port");
  return address.port;
}

// A separate app process per visit exercises startup with the same data directory.
async function openEngines(
  work: string,
  visit: (session: WebdriverIO.Browser) => Promise<void>,
) {
  const session = await startWdioSession(
    {
      browserName: "tauri",
      "tauri:options": { application: join(target, "debug", "zobrist-app") },
    },
    {
      driverProvider: "embedded",
      embeddedPort: await freePort(),
      env: {
        XDG_DATA_HOME: join(work, "data"),
        XDG_CACHE_HOME: join(work, "cache"),
      },
    },
  );
  try {
    await session.switchToWindow("main");
    await session.$("nav").$("aria/Engines").click();
    await visit(session);
  } finally {
    await cleanupWdioSession(session);
  }
}

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
