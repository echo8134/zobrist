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

// A separate app process per visit exercises startup with the same data directory.
export async function visitPage(
  work: string,
  page: string,
  visit: (session: WebdriverIO.Browser) => Promise<void>,
  env: Record<string, string> = {},
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
        ...env,
        XDG_DATA_HOME: join(work, "data"),
        XDG_CACHE_HOME: join(work, "cache"),
      },
    },
  );
  try {
    await session.switchToWindow("main");
    await session.$("nav").$(`aria/${page}`).click();
    await visit(session);
  } finally {
    await cleanupWdioSession(session);
  }
}
