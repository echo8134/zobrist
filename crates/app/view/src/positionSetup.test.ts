import { describe, expect, it } from "vitest";
import { changePosition, editFen, fenOf, startSetup } from "./positionSetup";

describe("the FEN field of a position being set up", () => {
  const setup = startSetup("4k3/4p3/8/8/8/8/8/4K3 b - - 9 12");

  it("preserves move counters when editing the position", () => {
    const changed = changePosition(setup, { turn: "white", enPassant: "d6" });
    expect(fenOf(changed.position)).toBe("4k3/4p3/8/8/8/8/8/4K3 w - d6 9 12");
  });

  it.each([
    "8/8/8/8/8/8/8 w - - 0 1",
    "8/8/8/8/8/8/8/7Kp w - - 0 1",
    "8/8/8/8/8/8/8/7X w - - 0 1",
    "8/8/8/8/8/8/8/8 w KK - 0 1",
    "8/8/8/8/8/8/8/8 black - - 0 1",
    "8/8/8/8/8/8/8/8 w - i4 0 1",
    "8/8/8/8/8/8/8/8 w - - 0",
  ])("keeps the position while the field contains %s", (text) => {
    const changed = editFen(setup, text);
    expect(changed.invalid).toEqual({ field: "fen", text });
    expect(fenOf(changed.position)).toBe("4k3/4p3/8/8/8/8/8/4K3 b - - 9 12");
  });

  it("accepts a complete FEN with an empty board for Rust to judge", () => {
    const changed = editFen(setup, "8/8/8/8/8/8/8/8 w Kq e3 0 1");
    expect(changed.invalid).toBeNull();
    expect(fenOf(changed.position)).toBe("8/8/8/8/8/8/8/8 w Kq e3 0 1");
  });
});
