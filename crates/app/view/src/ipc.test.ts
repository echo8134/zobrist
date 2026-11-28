import { expect, expectTypeOf, test, vi } from "vitest";
import { answersOnTheirWay, ask, finishRequests, prepareToClose } from "./ipc";

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

test("the answers about a game are on their way until their senders have handled them", async () => {
  let answerTheTag = (_response: unknown) => {};
  invoke.mockReturnValueOnce(
    new Promise((resolve) => {
      answerTheTag = resolve;
    }),
  );
  const steps: string[] = [];
  expect(answersOnTheirWay(7)).toBeUndefined();

  const tag = ask({ type: "SetTag", game: 7, tag: "White", text: "Epsilon" });
  const drawn = tag.then(() => steps.push("the sender handled the answer"));
  const waited = answersOnTheirWay(7)?.then(() => steps.push("answered"));

  expect(waited).toBeInstanceOf(Promise);
  // Another game has no request on its way.
  expect(answersOnTheirWay(8)).toBeUndefined();
  expect(invoke.mock.calls).toEqual([
    [
      "answer",
      { request: { type: "SetTag", game: 7, tag: "White", text: "Epsilon" } },
    ],
  ]);

  answerTheTag({ type: "SetTag", picture: null });
  await Promise.all([drawn, waited]);

  expect(steps).toEqual(["the sender handled the answer", "answered"]);
  expect(answersOnTheirWay(7)).toBeUndefined();
});

test("a request that Rust could not answer holds up nothing", async () => {
  invoke.mockRejectedValueOnce(new Error("no answer"));

  await expect(ask({ type: "GoTo", game: 9, place: [] })).rejects.toThrow(
    "no answer",
  );
  await answersOnTheirWay(9);

  expect(answersOnTheirWay(9)).toBeUndefined();
});

test("quitting waits for a new board and a save that was waiting for a field", async () => {
  let finishTag = () => {};
  let finishOpen = () => {};
  let finishSave = () => {};
  invoke.mockReturnValueOnce(
    new Promise<void>((resolve) => {
      finishTag = resolve;
    }),
  );
  invoke.mockReturnValueOnce(
    new Promise<void>((resolve) => {
      finishOpen = resolve;
    }),
  );
  invoke.mockReturnValueOnce(
    new Promise<void>((resolve) => {
      finishSave = resolve;
    }),
  );
  const steps: string[] = [];
  void ask({ type: "SetTag", game: 7, tag: "White", text: "Epsilon" });
  void ask({ type: "NewGame" }).then(() => steps.push("opened"));
  void answersOnTheirWay(7)?.then(async () => {
    await ask({ type: "SaveGame", game: 7 });
    steps.push("saved");
  });

  const quit = finishRequests("all").then(() => steps.push("quit"));
  finishTag();
  await answersOnTheirWay(7);
  expect(steps).toEqual([]);

  finishSave();
  await answersOnTheirWay(7);
  expect(steps).toEqual(["saved"]);

  finishOpen();
  await quit;
  expect(steps).toEqual(["saved", "opened", "quit"]);
});

test("a close waits for an import even if Rust receives the close request first", async () => {
  let finishImport = () => {};
  invoke
    .mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishImport = resolve;
      }),
    )
    .mockResolvedValueOnce({ type: "PrepareToClose", message: null })
    .mockResolvedValueOnce({
      type: "PrepareToClose",
      message: "Still importing.",
    })
    .mockResolvedValueOnce({ type: "PrepareToClose", message: null });
  void ask({
    type: "ImportDatabase",
    source: "a.pgn",
    destination: "a.zobrist",
  });
  const showWaiting = vi.fn();
  const closing = prepareToClose(new AbortController().signal, showWaiting);
  try {
    await vi.waitFor(() =>
      expect(showWaiting).toHaveBeenCalledWith("Still importing."),
    );
  } finally {
    finishImport();
    await closing;
  }
});

test("closing twice abandons the task immediately and still waits for the pending board edit", async () => {
  let finishEdit = () => {};
  let finishImport = () => {};
  invoke
    .mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishEdit = resolve;
      }),
    )
    .mockReturnValueOnce(
      new Promise<void>((resolve) => {
        finishImport = resolve;
      }),
    )
    .mockResolvedValueOnce({ type: "PrepareToClose", message: null });
  void ask({ type: "SetTag", game: 7, tag: "White", text: "Epsilon" });
  void ask({
    type: "ImportDatabase",
    source: "a.pgn",
    destination: "a.zobrist",
  });
  const second = new AbortController();
  const readyToSaveBoards = vi.fn();
  const closing = prepareToClose(second.signal, vi.fn()).then(
    readyToSaveBoards,
  );
  second.abort();
  try {
    await vi.waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("answer", {
        request: { type: "PrepareToClose", abandon: true },
      }),
    );
    expect(readyToSaveBoards).not.toHaveBeenCalled();
    finishEdit();
    await closing;
    expect(readyToSaveBoards).toHaveBeenCalledOnce();
  } finally {
    finishEdit();
    finishImport();
  }
});

test("a game can be saved while the tablebase is still answering", async () => {
  let refuse = (_reason: unknown) => {};
  invoke.mockReturnValueOnce(
    new Promise((_resolve, reject) => {
      refuse = reject;
    }),
  );
  const lookup = ask({ type: "LookupTablebase", game: 4, revision: 2 });
  expect(answersOnTheirWay(4)).toBeUndefined();
  await finishRequests("boards");
  await finishRequests("all");
  refuse(new Error("offline"));
  await expect(lookup).rejects.toThrow("offline");
});

test("a checkpoint waits for a game opening from a collection", async () => {
  vi.useFakeTimers();
  let finishOpen = () => {};
  invoke.mockReturnValueOnce(
    new Promise<void>((resolve) => {
      finishOpen = resolve;
    }),
  );
  try {
    const steps: string[] = [];
    const opening = ask({
      type: "OpenDatabaseGame",
      path: "Study.zobrist",
      number: 2,
    }).then(() => steps.push("opened"));
    const checkpoint = finishRequests("boards").then(() =>
      steps.push("checkpoint"),
    );
    await vi.runAllTimersAsync();
    finishOpen();
    await opening;
    await vi.runAllTimersAsync();
    await checkpoint;
    expect(steps).toEqual(["opened", "checkpoint"]);
  } finally {
    vi.useRealTimers();
  }
});
