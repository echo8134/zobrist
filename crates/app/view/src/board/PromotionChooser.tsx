import { createElement, useEffect } from "react";
import type { Promotion } from "../generated/Promotion";
import type { Side } from "../generated/Side";
import styles from "./PromotionChooser.module.css";

const { promotionChooser, fromBottom, choice, choicePiece } = styles;

const choices: readonly { piece: Promotion; name: string }[] = [
  { piece: "queen", name: "Queen" },
  { piece: "knight", name: "Knight" },
  { piece: "rook", name: "Rook" },
  { piece: "bishop", name: "Bishop" },
];

type Props = {
  side: Side;
  targetSquare: string;
  bottomSide: Side;
  onChoose: (piece: Promotion) => void;
  onCancel: () => void;
};

export function PromotionChooser({
  side,
  targetSquare,
  bottomSide,
  onChoose,
  onCancel,
}: Props) {
  useEffect(() => {
    const cancelOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onCancel();
      }
    };
    document.addEventListener("keydown", cancelOnEscape);
    return () => document.removeEventListener("keydown", cancelOnEscape);
  }, [onCancel]);

  const columnFromFileA = "abcdefgh".indexOf(targetSquare.charAt(0));
  const columnFromLeft =
    bottomSide === "white" ? columnFromFileA : 7 - columnFromFileA;
  const startsAtTop =
    (targetSquare.charAt(1) === "8") === (bottomSide === "white");
  return (
    <fieldset
      // The stylesheet of the board component gives a piece its picture only inside an element
      // with the class cg-wrap.
      className={`cg-wrap ${promotionChooser} ${startsAtTop ? "" : fromBottom}`}
      style={{ left: `${columnFromLeft * 12.5}%` }}
    >
      {choices.map(({ piece, name }) => (
        <button
          key={piece}
          type="button"
          className={choice}
          aria-label={name}
          onClick={() => onChoose(piece)}
        >
          {createElement("piece", {
            className: `${choicePiece} ${side} ${piece}`,
          })}
        </button>
      ))}
    </fieldset>
  );
}
