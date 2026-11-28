import { expect, test, vi } from "vitest";
import { followAnalysis } from "./EngineAnalysis";
import type { Response } from "./generated/Response";

const ask = vi.hoisted(() => vi.fn());
vi.mock("./ipc", () => ({ ask }));

type Answer = Extract<Response, { type: "Analysis" }>;

test("a poll from the first visit cannot put its lines on the board after the player returns", async () => {
  vi.useFakeTimers();
  const shown: Answer["snapshot"][] = [];
  let answer = (_response: Answer) => {};
  ask.mockImplementation(
    () =>
      new Promise<Answer>((resolve) => {
        answer = resolve;
      }),
  );
  const search = followAnalysis(7, { revision: 10, place: [] }, (snapshot) =>
    shown.push(snapshot),
  );
  try {
    const start = shown[0];
    if (!start) throw new Error("The board did not start the engine.");
    const { generation } = start;
    function lines(revision: number, evaluation: string): Answer {
      return {
        type: "Analysis",
        snapshot: {
          game: 7,
          revision,
          place: [],
          generation,
          state: {
            type: "Searching",
            lines: [
              { evaluation, notation: "1. e4 e5", moves: ["e2e4", "e7e5"] },
            ],
          },
        },
      };
    }

    answer(lines(10, "+0.34"));
    await Promise.resolve();
    await vi.advanceTimersToNextTimerAsync();
    expect(shown.at(-1)).toEqual(lines(10, "+0.34").snapshot);
    expect(ask).toHaveBeenLastCalledWith({
      type: "Analysis",
      game: 7,
      action: "Status",
      generation,
    });

    search.follow({ revision: 11, place: [0] });
    search.follow({ revision: 12, place: [] });
    expect(shown.at(-1)?.state).toEqual({ type: "Searching", lines: [] });
    expect(ask).toHaveBeenCalledTimes(2);
    answer(lines(10, "+9.99"));
    await Promise.resolve();
    expect(shown.at(-1)?.state).toEqual({ type: "Searching", lines: [] });
    expect(ask).toHaveBeenCalledTimes(3);
    answer(lines(12, "+0.41"));
    await Promise.resolve();
    expect(shown.at(-1)).toEqual(lines(12, "+0.41").snapshot);

    await vi.advanceTimersToNextTimerAsync();
    const last = shown.length;
    const late = answer;
    search.stop();
    expect(ask).toHaveBeenLastCalledWith({
      type: "Analysis",
      game: 7,
      action: "Stop",
      generation,
    });
    late(lines(12, "+8.88"));
    await Promise.resolve();
    expect(shown).toHaveLength(last);
    expect(vi.getTimerCount()).toBe(0);
  } finally {
    search.stop();
    vi.useRealTimers();
  }
});
