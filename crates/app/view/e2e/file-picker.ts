import { browser, expect } from "@wdio/globals";

type Choice = "File" | "Folder" | "New" | "Engine";

/** WebDriver cannot drive the system dialog. Only its next answer is supplied by the test. */
export async function answerNextPicker(
  type:
    | "PickPgnPath"
    | "PickDatabasePath"
    | "PickNewDatabasePath"
    | "PickEnginePath",
  path: string | null,
  session: WebdriverIO.Browser = browser,
) {
  await session.execute(
    `
      const [type, path] = arguments;
      const original = window.fetch;
      const endpoint = window.__TAURI_INTERNALS__.convertFileSrc("answer", "ipc");
      window.fetch = function(input, options) {
        if (String(input) === endpoint && typeof options?.body === "string") {
          const request = JSON.parse(options.body).request;
          if (request?.type === type) {
            window.fetch = original;
            window.pickerRequest = request;
            return Promise.resolve(Response.json({ type, path }, {
              headers: { "Tauri-Response": "ok" },
            }));
          }
        }
        return original.call(window, input, options);
      };
      delete window.pickerRequest;
    `,
    type,
    path,
  );
}

export async function pick(
  choice: Choice,
  path: string | null,
  session: WebdriverIO.Browser = browser,
) {
  const requestType = choice === "Engine" ? "PickEnginePath" : "PickPgnPath";
  await answerNextPicker(requestType, path, session);
  if (choice === "Engine") {
    await session.$("main").$("aria/Add engine…").click();
  } else if (choice === "New") {
    await session.$("main").$("aria/New PGN…").click();
  } else {
    await session.$("main").$("aria/Open").click();
    await session
      .$("main")
      .$(choice === "File" ? "aria/PGN file…" : "aria/Folder…")
      .click();
  }
  await session.waitUntil(async () =>
    Boolean(await session.execute("return window.pickerRequest")),
  );
  if (choice === "Engine") return;
  expect(await session.execute("return window.pickerRequest.choice")).toBe(
    choice,
  );
  return session.execute("return window.pickerRequest.folder");
}

export async function startImport(
  app: WebdriverIO.Browser,
  source: string | null,
  destination: string | null,
) {
  // The second dialog opens immediately after the first answer. Arm its reply first.
  if (source !== null) {
    await answerNextPicker("PickNewDatabasePath", destination, app);
  }
  await answerNextPicker("PickPgnPath", source, app);
  const button = app.$("button=Import PGN…");
  await button.click();
  const lastPicker = source === null ? "PickPgnPath" : "PickNewDatabasePath";
  await app.waitUntil(() =>
    app.execute<boolean, [string]>(
      "return window.pickerRequest?.type === arguments[0]",
      lastPicker,
    ),
  );
}

export async function importPgn(
  app: WebdriverIO.Browser,
  source: string | null,
  destination: string | null,
) {
  await startImport(app, source, destination);
  await expect(app.$("button=Import PGN…")).toBeEnabled();
}
