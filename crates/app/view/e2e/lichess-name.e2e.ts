import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import type { Server } from "node:net";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { withDesktopApp } from "./desktop-session";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));

async function listenLocal(server: Server) {
  await new Promise<void>((ready, reject) => {
    server.once("error", reject).listen(0, "127.0.0.1", ready);
  });
  const address = server.address();
  if (!address || typeof address === "string")
    throw new Error("Expected a TCP port");
  return address.port;
}

describe("the Lichess name on this device", () => {
  it("remembers a saved name and its removal across app restarts without a site request", async () => {
    const data = mkdtempSync(join(target, "e2e", "lichess-name-"));
    const requests: string[] = [];
    const proxy = createServer((request, response) => {
      requests.push(request.url ?? "");
      response.writeHead(502).end();
    });
    proxy.on("connect", (request, socket) => {
      requests.push(request.url ?? "");
      socket.end("HTTP/1.1 502 Bad Gateway\r\n\r\n");
    });
    const proxyUrl = `http://127.0.0.1:${await listenLocal(proxy)}`;
    async function openUser(app: WebdriverIO.Browser) {
      await app.$("nav").$("aria/User").click();
      await app.$("main summary").click();
    }

    function onUser(check: (app: WebdriverIO.Browser) => Promise<void>) {
      return withDesktopApp(
        data,
        async (app) => {
          await openUser(app);
          await check(app);
        },
        {
          XDG_CACHE_HOME: data,
          http_proxy: proxyUrl,
          https_proxy: proxyUrl,
          HTTP_PROXY: proxyUrl,
          HTTPS_PROXY: proxyUrl,
          no_proxy: "localhost,127.0.0.1",
          NO_PROXY: "localhost,127.0.0.1",
        },
      );
    }

    const field = (app: WebdriverIO.Browser) => app.$("aria/Lichess username");
    const save = (app: WebdriverIO.Browser) => app.$("button=Save");
    const saved = (app: WebdriverIO.Browser) =>
      app.$("main summary span:last-of-type");

    try {
      await onUser(async (app) => {
        await expect(field(app)).toBeEnabled();
        await expect(field(app)).toHaveValue("");
        await field(app).setValue("  RookExample  ");
        await save(app).click();
        await expect(saved(app)).toHaveText("RookExample");
      });
      expect(
        JSON.parse(
          readFileSync(
            join(data, "io.github.echo8134.zobrist", "accounts.json"),
            "utf8",
          ),
        ),
      ).toEqual({ lichessUsername: "RookExample" });

      await onUser(async (app) => {
        await expect(field(app)).toHaveValue("RookExample");
        await expect(saved(app)).toHaveText("RookExample");
        const picture = await app.saveScreenshot(
          join(target, "e2e", "user-lichess-name.png"),
        );
        expect([picture.readUInt32BE(16), picture.readUInt32BE(20)]).toEqual([
          1280, 800,
        ]);
        await field(app).setValue("KnightExample");
        await save(app).click();
        await expect(saved(app)).toHaveText("KnightExample");
        await app.$("nav").$("aria/Home").click();
        await openUser(app);
        await expect(field(app)).toHaveValue("KnightExample");
        await field(app).setValue("");
        await save(app).click();
        await expect(app.$('[role="alert"]')).toHaveText(
          "Enter a Lichess username.",
        );
        await app.$("nav").$("aria/Home").click();
        await openUser(app);
        await expect(field(app)).toHaveValue("KnightExample");
        await app.$("button=Remove name").click();
        await expect(saved(app)).toHaveText("Not connected");
      });
      await onUser(async (app) => {
        await expect(field(app)).toBeEnabled();
        await expect(field(app)).toHaveValue("");
        await expect(saved(app)).toHaveText("Not connected");
      });
      expect(requests).toEqual([]);
    } finally {
      proxy.close();
      rmSync(data, { recursive: true, force: true });
    }
  });
});
