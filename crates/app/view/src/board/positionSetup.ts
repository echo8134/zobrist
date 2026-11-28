import type { PositionDraft } from "../generated/PositionDraft";
import type { PositionFields } from "../generated/PositionFields";
import type { Side } from "../generated/Side";
import { ask } from "../ipc";
import type { Change, Tab } from "./tabs";

export const roles = [
  "king",
  "queen",
  "rook",
  "bishop",
  "knight",
  "pawn",
] as const;
type Piece = { color: Side; role: (typeof roles)[number] };
type TypedField = { field: "fen" | "enPassant"; text: string };
type Description = { position: PositionDraft; raw: TypedField | null };
export type PositionSetup = Description & {
  tool: Piece | "eraser" | null;
  pending?: Promise<Description>;
};
export type SetupEdit =
  | { type: "Fen"; fen: string }
  | { type: "Fields"; fields: Partial<PositionFields> };

export async function editSetup(
  game: number,
  edit: SetupEdit,
  tabNow: (game: number) => Pick<Tab, "panel"> | undefined,
  change: (change: Change) => void,
) {
  const panel = tabNow(game)?.panel;
  if (panel?.type !== "setup" || panel.status === "pending") return;
  const setup = panel.draft;
  const raw: TypedField | null =
    edit.type === "Fen"
      ? { field: "fen", text: edit.fen }
      : edit.fields.enPassant !== undefined
        ? { field: "enPassant", text: edit.fields.enPassant ?? "" }
        : null;

  // Each change starts from the draft Rust accepted for the preceding edit.
  const pending = Promise.resolve(setup.pending ?? setup).then(
    async ({ position }) => {
      const input =
        edit.type === "Fen"
          ? edit
          : { type: edit.type, fields: { ...position.fields, ...edit.fields } };
      const { draft } = await ask({ type: "DescribePosition", input });
      return draft === null
        ? { position, raw }
        : { position: draft, raw: null };
    },
  );
  change({
    type: "setPanel",
    game,
    panel: { type: "setup", draft: { ...setup, raw, pending } },
  });
  const description = await pending;
  const current = tabNow(game)?.panel;
  if (current?.type === "setup" && current.draft.pending === pending) {
    change({
      type: "setPanel",
      game,
      panel: {
        ...current,
        draft: { ...description, tool: current.draft.tool },
      },
    });
  }
}
