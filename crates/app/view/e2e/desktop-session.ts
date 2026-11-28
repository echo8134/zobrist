import { createServer } from "node:net";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { cleanupWdioSession, startWdioSession } from "@wdio/tauri-service";

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

export async function withDesktopApp(
  data: string,
  visit: (session: WebdriverIO.Browser) => Promise<void>,
  env: Record<string, string> = {},
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
  try {
    await session.switchToWindow("main");
    await visit(session);
  } finally {
    try {
      const endpoint = `http://127.0.0.1:${port}`;
      await requestDriver(
        `${endpoint}/session/${session.sessionId}/window`,
        "DELETE",
      );
      await session.waitUntil(
        async () => !(await requestDriver(`${endpoint}/status`, "GET")),
        { timeoutMsg: "The app is still running after closing its window" },
      );
    } finally {
      await cleanupWdioSession(session);
    }
  }
}

export function visitPage(
  work: string,
  page: string,
  visit: (session: WebdriverIO.Browser) => Promise<void>,
  env: Record<string, string> = {},
) {
  return withDesktopApp(
    join(work, "data"),
    async (session) => {
      await session.$("nav").$(`aria/${page}`).click();
      await visit(session);
    },
    { ...env, XDG_CACHE_HOME: join(work, "cache") },
  );
}
