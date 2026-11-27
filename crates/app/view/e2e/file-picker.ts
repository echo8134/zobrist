import { browser as defaultBrowser, expect } from "@wdio/globals";

type Choice = "File" | "Folder" | "New";

/** WebDriver cannot drive the system dialog. Only its next answer is supplied by the test. */
export async function pick(
  choice: Choice,
  path: string | null,
  browser = defaultBrowser,
) {
  await browser.execute(
    `
      const [path] = arguments;
      const original = window.fetch;
      const endpoint = window.__TAURI_INTERNALS__.convertFileSrc("answer", "ipc");
      window.fetch = function(input, options) {
        if (String(input) === endpoint && typeof options?.body === "string") {
          const request = JSON.parse(options.body).request;
          if (request?.type === "PickPgnPath") {
            window.fetch = original;
            window.pickerRequest = request;
            return Promise.resolve(Response.json({ type: "PickPgnPath", path }, {
              headers: { "Tauri-Response": "ok" },
            }));
          }
        }
        return original.call(window, input, options);
      };
      delete window.pickerRequest;
    `,
    path,
  );
  if (choice === "New") {
    await browser.$("main").$("aria/New PGN…").click();
  } else {
    await browser.$("main").$("aria/Open").click();
    await browser
      .$("main")
      .$(choice === "File" ? "aria/PGN file…" : "aria/Folder…")
      .click();
  }
  await browser.waitUntil(async () =>
    Boolean(await browser.execute("return window.pickerRequest")),
  );
  expect(await browser.execute("return window.pickerRequest.choice")).toBe(
    choice,
  );
  return browser.execute("return window.pickerRequest.folder");
}
