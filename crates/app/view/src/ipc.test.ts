import { expect, expectTypeOf, test, vi } from "vitest";
import { answered, ask } from "./ipc";

// The stand-in for Rust. Only the module under test imports the real function.
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

// These two calls never run, because Rust answers only inside the app. The TypeScript check
// judges them.

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

test("the requests about a game are answered when their senders have handled the answers", async () => {
  let answerTheTag = (_response: unknown) => {};
  invoke.mockReturnValueOnce(
    new Promise((resolve) => {
      answerTheTag = resolve;
    }),
  );
  const steps: string[] = [];

  const tag = ask({ type: "SetTag", game: 7, tag: "White", text: "Epsilon" });
  const drawn = tag.then(() => steps.push("the sender handled the answer"));
  const waited = answered(7).then(() => steps.push("answered"));

  // Another game has no request on its way.
  await answered(8);
  expect(steps).toEqual([]);
  expect(invoke.mock.calls).toEqual([
    [
      "answer",
      { request: { type: "SetTag", game: 7, tag: "White", text: "Epsilon" } },
    ],
  ]);

  answerTheTag({ type: "SetTag", picture: null });
  await Promise.all([drawn, waited]);

  expect(steps).toEqual(["the sender handled the answer", "answered"]);
});

test("a request that Rust could not answer holds up nothing", async () => {
  invoke.mockRejectedValueOnce(new Error("no answer"));

  await expect(ask({ type: "GoTo", game: 9, place: [] })).rejects.toThrow(
    "no answer",
  );
  await answered(9);
});
