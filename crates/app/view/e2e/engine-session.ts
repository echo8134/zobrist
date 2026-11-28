import { visitPage } from "./desktop-session";

export function openEngines(
  work: string,
  visit: (session: WebdriverIO.Browser) => Promise<void>,
) {
  return visitPage(work, "Engines", visit);
}
