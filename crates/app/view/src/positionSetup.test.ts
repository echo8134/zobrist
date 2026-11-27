import { expect, test, vi } from "vitest";
import type { PositionDraft } from "./generated/PositionDraft";
import { ask } from "./ipc";
import { editSetup } from "./positionSetup";
import type { Change, Tab } from "./tabs";

vi.mock("./ipc", () => ({ ask: vi.fn() }));

const position: PositionDraft = {
  fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
  fields: {
    pieces: "4k3/8/8/8/8/8/8/4K3",
    turn: "white",
    whiteShort: false,
    whiteLong: false,
    blackShort: false,
    blackLong: false,
    enPassant: null,
  },
};

function openSetup() {
  let panel: Tab["panel"] = {
    type: "setup",
    draft: { position, raw: null, tool: "eraser" },
  };
  const tabNow = () => ({ panel });
  const change = (next: Change) => {
    if (next.type === "setPanel") panel = next.panel;
  };
  return { tabNow, change };
}

test("two quick edits keep the first accepted position when Rust refuses the second", async () => {
  const { tabNow, change } = openSetup();
  const blackToPlay: PositionDraft = {
    fen: "4k3/8/8/8/8/8/8/4K3 b - - 0 1",
    fields: { ...position.fields, turn: "black" },
  };
  vi.mocked(ask)
    .mockResolvedValueOnce({ type: "DescribePosition", draft: blackToPlay })
    .mockResolvedValueOnce({ type: "DescribePosition", draft: null });
  const first = editSetup(
    1,
    { type: "Fen", fen: blackToPlay.fen },
    tabNow,
    change,
  );
  const second = editSetup(
    1,
    { type: "Fields", fields: { enPassant: "e" } },
    tabNow,
    change,
  );
  await Promise.all([first, second]);

  expect(ask).toHaveBeenLastCalledWith({
    type: "DescribePosition",
    input: {
      type: "Fields",
      fields: {
        pieces: "4k3/8/8/8/8/8/8/4K3",
        turn: "black",
        whiteShort: false,
        whiteLong: false,
        blackShort: false,
        blackLong: false,
        enPassant: "e",
      },
    },
  });
  expect(tabNow().panel).toEqual({
    type: "setup",
    draft: {
      position: blackToPlay,
      raw: { field: "enPassant", text: "e" },
      tool: "eraser",
    },
  });
});

test("an answer that arrives after Cancel leaves setup closed", async () => {
  const { tabNow, change } = openSetup();
  vi.mocked(ask).mockResolvedValueOnce({
    type: "DescribePosition",
    draft: position,
  });
  const editing = editSetup(
    1,
    { type: "Fen", fen: position.fen },
    tabNow,
    change,
  );
  change({ type: "setPanel", game: 1, panel: undefined });
  await editing;
  expect(tabNow().panel).toBeUndefined();
});
