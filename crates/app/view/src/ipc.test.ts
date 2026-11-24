import { expectTypeOf, test } from "vitest";
import { ask } from "./ipc";

// These calls never run, because Rust answers only inside the app. The TypeScript check judges them.

test("a version request resolves with the version response", () => {
  const askForTheVersion = () => ask({ type: "Version" });
  expectTypeOf(askForTheVersion).returns.resolves.toEqualTypeOf<{
    type: "Version";
    version: string;
  }>();
});

test("a request that Rust does not know does not compile", () => {
  // @ts-expect-error Rust has no request of the type Unknown.
  const askForSomethingUnknown = () => ask({ type: "Unknown" });
  expectTypeOf(askForSomethingUnknown).toBeFunction();
});
