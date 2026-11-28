import { once } from "node:events";
import { createServer } from "node:net";
import { fileURLToPath } from "node:url";
import { cleanupWdioSession, startWdioSession } from "@wdio/tauri-service";

const target = fileURLToPath(new URL("../../../../target", import.meta.url));

async function availablePort() {
  // The normal test window already owns its driver's port.
  await using listener = createServer().listen(0, "127.0.0.1");
  await once(listener, "listening");
  const address = listener.address();
  if (!address || typeof address === "string") throw new Error("No TCP port");
  return address.port;
}

export async function withApp(
  data: string,
  check: (app: WebdriverIO.Browser) => Promise<void>,
) {
  const app = await startWdioSession({
    "tauri:options": { application: `${target}/debug/zobrist-app` },
    "wdio:tauriServiceOptions": {
      embeddedPort: await availablePort(),
      env: { XDG_DATA_HOME: data, XDG_CACHE_HOME: `${data}/cache` },
    },
  });
  try {
    await app.switchToWindow("main");
    await check(app);
  } finally {
    await cleanupWdioSession(app);
  }
}
