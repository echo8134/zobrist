import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { openEngines } from "./engine-session";
import { pick } from "./file-picker";

const screenshots = fileURLToPath(
  new URL("../../../../target/e2e", import.meta.url),
);
const original = "1. e4 e5 2. Nf3 *\n";
const added = "1. e4 e5 (1... c5 2. Nf3) 2. Nf3 *\n";
const sicilian = "1. e4 c5 2. Nf3";

// Delay delivery of the next click. Its original payload and the eventual Rust answer
// travel through the same command as an ordinary insertion.
async function holdAddition(session: WebdriverIO.Browser) {
  await session.execute(`
    const original = window.fetch;
    const endpoint = window.__TAURI_INTERNALS__.convertFileSrc("answer", "ipc");
    delete window.releaseAddition;
    window.fetch = function(input, options) {
      if (String(input) === endpoint && typeof options?.body === "string" &&
          JSON.parse(options.body).request.type === "AddEngineVariation") {
        window.fetch = original;
        return new Promise((resolve, reject) => {
          window.releaseAddition = async () => {
            try {
              const response = await original.call(window, input, options);
              const answer = await response.clone().json();
              resolve(response);
              return answer.answer.type;
            } catch (error) { reject(error); throw error; }
          };
        });
      }
      return original.call(window, input, options);
    };`);
}

describe("an engine line in the notation", () => {
  it("keeps the clicked moves as one edit and refuses an obsolete search", async function () {
    this.timeout(60_000);
    mkdirSync(screenshots, { recursive: true });
    const work = mkdtempSync(join(screenshots, "variation-"));
    const program = join(work, "variation");
    const file = join(work, "opening.pgn");
    try {
      copyFileSync(new URL("./analysis-engine.sh", import.meta.url), program);
      chmodSync(program, 0o700);
      writeFileSync(file, original);
      await openEngines(work, async (session) => {
        const tool = (name: string) => session.$("main").$(`aria/${name}`);
        const row = (rank = 2) =>
          session.$(
            `section[aria-label="Engine analysis"] li:nth-child(${rank})`,
          );
        const insert = (rank = 2) =>
          row(rank).$("button=Add variation").click();
        const exportGame = () =>
          session.execute(`
          return window.__TAURI_INTERNALS__.invoke("answer", {request:{type:"GamePgn", game:1}})
            .then(answer => answer.text);
        `);
        async function holdClick(rank = 2) {
          await holdAddition(session);
          await insert(rank);
          await session.waitUntil(async () =>
            Boolean(
              await session.execute(
                "return window.releaseAddition !== undefined",
              ),
            ),
          );
        }
        const release = () =>
          session.execute("return window.releaseAddition()");
        await pick("Engine", program, session);
        await expect(session.$("main li").$("span=Default")).toBeDisplayed();
        await session.$("nav").$("aria/Files").click();
        await pick("File", file, session);
        await session.$("main table tbody tr").click();
        await tool("Start engine").click();
        await expect(row().$("span")).toHaveText(sicilian);
        await holdClick(1);
        expect(await release()).toBe("Present");
        await expect(tool("Save")).not.toBeEnabled();
        await expect(tool("Undo")).not.toBeEnabled();
        expect(await exportGame()).toBe(original);
        await insert();
        await expect(tool("c5")).toBeDisplayed();
        await expect(tool("Save")).toBeEnabled();
        await expect(session.$("header [role=tab]")).toHaveAttribute(
          "aria-label",
          expect.stringContaining("not saved"),
        );
        await expect(
          session.$$("main aside button[aria-current=true]"),
        ).toBeElementsArrayOfSize(0);
        expect(await exportGame()).toBe(added);
        const image = await session.saveScreenshot(
          join(screenshots, "engine-variation.png"),
        );
        expect([image.readUInt32BE(16), image.readUInt32BE(20)]).toEqual([
          1280, 800,
        ]);
        await holdClick();
        expect(await release()).toBe("Present");
        expect(await exportGame()).toBe(added);
        await tool("Undo").click();
        await expect(tool("Save")).not.toBeEnabled();
        expect(await exportGame()).toBe(original);
        await tool("Redo").click();
        await expect(tool("Save")).toBeEnabled();
        expect(await exportGame()).toBe(added);
        await tool("Undo").click();
        await expect(tool("Save")).not.toBeEnabled();

        await tool("Stop engine").click();
        writeFileSync(`${program}.rerank`, "");
        await tool("Start engine").click();
        await expect(row().$("span")).toHaveText(sicilian);
        await holdClick();
        rmSync(`${program}.rerank`);
        await expect(row().$("span")).toHaveText("1. d4 d5");
        expect(await release()).toBe("Added");
        await expect(tool("Save")).toBeEnabled();
        expect(await exportGame()).toBe(added);
        await tool("Undo").click();
        await expect(tool("Save")).not.toBeEnabled();

        await holdClick();
        await tool("Stop engine").click();
        await tool("Start engine").click();
        await expect(row().$("span")).toHaveText(sicilian);
        expect(await release()).toBe("Refused");
        await holdClick();
        await tool("e4").click();
        await expect(tool("e4")).toHaveAttribute("aria-current", "true");
        await session.keys("ArrowLeft");
        await expect(row().$("span")).toHaveText(sicilian);
        expect(await release()).toBe("Refused");
        expect(await exportGame()).toBe(original);
        await expect(tool("Save")).not.toBeEnabled();
        await expect(tool("Undo")).not.toBeEnabled();
        await expect(tool("Redo")).toBeEnabled();
        expect(readFileSync(file, "utf8")).toBe(original);
        await tool("Stop engine").click();
      });
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
});
