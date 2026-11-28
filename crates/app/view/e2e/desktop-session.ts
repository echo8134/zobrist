import { spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanupWdioSession, startWdioSession } from "@wdio/tauri-service";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));

export async function freePort() {
  const socket = createServer();
  await new Promise<void>((ready, reject) => {
    socket.once("error", reject).listen(0, "127.0.0.1", ready);
  });
  const address = socket.address();
  await new Promise<void>((done) => socket.close(() => done()));
  if (!address || typeof address === "string") throw Error("Expected a port");
  return address.port;
}

// Only a refused connection proves that the embedded driver has stopped listening.
async function requestDriver(url: string, method: "GET" | "DELETE") {
  try {
    const response = await fetch(url, { method });
    if (!response.ok)
      throw new Error(`WebDriver ${method} ${url}: ${response.status}`);
    return true;
  } catch (error) {
    const cause = error instanceof TypeError ? error.cause : undefined;
    if (cause instanceof Error && "code" in cause) {
      if (cause.code === "ECONNREFUSED") return false;
      // Closing the last window can break the connection before the reply arrives.
      if (cause.code === "UND_ERR_SOCKET" || cause.code === "ECONNRESET")
        return true;
    }
    throw error;
  }
}

export type DesktopWindow = {
  close(): Promise<void>;
  waitForExit(timeout?: number): Promise<void>;
};

type Visit = (app: WebdriverIO.Browser, window: DesktopWindow) => Promise<void>;

export async function withDesktopApp(
  data: string,
  visit: Visit,
  env: Record<string, string> = {},
  exit: "close window" | "terminate process" = "close window",
) {
  const port = await freePort();
  const session = await startWdioSession(
    {
      browserName: "tauri",
      "tauri:options": { application: join(target, "debug", "zobrist-app") },
    },
    {
      driverProvider: "embedded",
      embeddedPort: port,
      env: { XDG_DATA_HOME: data, XDG_CACHE_HOME: join(data, "cache"), ...env },
    },
  );
  session.options.connectionRetryCount = 0;
  const endpoint = `http://127.0.0.1:${port}`;
  const window: DesktopWindow = {
    async close() {
      await requestDriver(
        `${endpoint}/session/${session.sessionId}/window`,
        "DELETE",
      );
    },
    async waitForExit(timeout = 5000) {
      await session.waitUntil(
        async () => {
          if (await requestDriver(`${endpoint}/status`, "GET")) return false;
          // The app holds this lock until exit, including after its last window closes.
          const lock = spawnSync("flock", [
            "--nonblock",
            join(data, "io.github.echo8134.zobrist", "app.lock"),
            "true",
          ]);
          if (lock.error) throw lock.error;
          return lock.status === 0;
        },
        {
          timeout,
          interval: 25,
          timeoutMsg: "The app is still running after closing its window",
        },
      );
    },
  };
  try {
    await session.switchToWindow("main");
    await visit(session, window);
  } finally {
    try {
      if (exit === "close window") {
        await window.close();
        await window.waitForExit();
      }
    } finally {
      await cleanupWdioSession(session);
    }
  }
}

export function visitPage(
  work: string,
  page: string,
  visit: Visit,
  env: Record<string, string> = {},
) {
  return withDesktopApp(
    join(work, "data"),
    async (session, window) => {
      await session.$("nav").$(`aria/${page}`).click();
      await visit(session, window);
    },
    { ...env, XDG_CACHE_HOME: join(work, "cache") },
  );
}
