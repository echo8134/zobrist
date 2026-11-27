import type { Side } from "./generated/Side";

export const startingFen =
  "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
export const emptyFen = "8/8/8/8/8/8/8/8 w - - 0 1";

export const roles = [
  "king",
  "queen",
  "rook",
  "bishop",
  "knight",
  "pawn",
] as const;
type Piece = { color: Side; role: (typeof roles)[number] };

type FenFields = {
  pieces: string;
  turn: Side;
  castling: string;
  enPassant: string;
  halfmoves: string;
  fullmoves: string;
};

export type PositionSetup = {
  position: FenFields;
  tool: Piece | "eraser" | null;
  invalid: { field: "fen" | "enPassant"; text: string } | null;
};

export function startSetup(fen: string): PositionSetup {
  const [
    pieces = "",
    side,
    castling = "-",
    enPassant = "-",
    halfmoves = "0",
    fullmoves = "1",
  ] = fen.trim().split(/\s+/);
  return {
    position: {
      pieces,
      turn: side === "b" ? "black" : "white",
      castling,
      enPassant,
      halfmoves,
      fullmoves,
    },
    tool: null,
    invalid: null,
  };
}

export function fenOf({
  pieces,
  turn,
  castling,
  enPassant,
  halfmoves,
  fullmoves,
}: FenFields) {
  return [
    pieces,
    turn === "white" ? "w" : "b",
    castling,
    enPassant,
    halfmoves,
    fullmoves,
  ].join(" ");
}

export function changePosition(
  setup: PositionSetup,
  fields: Partial<FenFields>,
): PositionSetup {
  return {
    ...setup,
    position: { ...setup.position, ...fields },
    invalid: null,
  };
}

export function editFen(setup: PositionSetup, text: string): PositionSetup {
  const parts = text.trim().split(/\s+/);
  const [
    pieces = "",
    side,
    castling = "",
    enPassant = "",
    halfmoves = "",
    fullmoves = "",
  ] = parts;
  const ranks = pieces.split("/");
  const hasEightSquares = (rank: string) =>
    /^[pnbrqk1-8]+$/i.test(rank) &&
    !/\d\d/.test(rank) &&
    [...rank].reduce(
      (squares, letter) => squares + (Number(letter) || 1),
      0,
    ) === 8;
  if (
    parts.length !== 6 ||
    ranks.length !== 8 ||
    !ranks.every(hasEightSquares) ||
    (side !== "w" && side !== "b") ||
    !(castling === "-" || /^(?=[KQkq])K?Q?k?q?$/.test(castling)) ||
    !(enPassant === "-" || /^[a-h][1-8]$/.test(enPassant)) ||
    !/^\d+$/.test(halfmoves) ||
    !/^[1-9]\d*$/.test(fullmoves)
  ) {
    return { ...setup, invalid: { field: "fen", text } };
  }
  return { ...startSetup(text), tool: setup.tool };
}
