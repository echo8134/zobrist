import { basename } from "node:path";
import { expect } from "@wdio/globals";

export const recent = (app: WebdriverIO.Browser) =>
  app.$('main section[aria-label="Recent files"]');

export async function expectRecent(app: WebdriverIO.Browser, paths: string[]) {
  const entries = recent(app).$$("button");
  await expect(entries).toBeElementsArrayOfSize(paths.length);
  for (const [index, path] of paths.entries()) {
    const entry = recent(app).$(`li:nth-child(${index + 1}) button`);
    const nameLabel = entry.$("span:first-child");
    const pathLabel = entry.$("span:last-child");
    await expect(nameLabel).toBeDisplayed();
    await expect(nameLabel).toHaveText(basename(path));
    await expect(pathLabel).toBeDisplayed();
    await expect(pathLabel).toHaveText(path);
  }
}
