import type { NotationMove } from "./generated/NotationMove";
import styles from "./Notation.module.css";

const { notation, line, step, moveNumber, moveText, variation } = styles;

type Props = {
  mainLine: readonly NotationMove[];
  currentPlace: readonly number[];
  onGoTo: (place: number[]) => void;
};

export function Notation({ mainLine, currentPlace, onGoTo }: Props) {
  return (
    <ol className={notation}>
      <Moves
        moves={mainLine}
        currentPlace={currentPlace.join()}
        onGoTo={onGoTo}
      />
    </ol>
  );
}

type MovesProps = {
  moves: readonly NotationMove[];
  currentPlace: string;
  onGoTo: (place: number[]) => void;
};

function Moves({ moves, currentPlace, onGoTo }: MovesProps) {
  return moves.map((move, index) => {
    const moveBefore = moves[index - 1];
    const numbered =
      move.side === "white" ||
      moveBefore === undefined ||
      moveBefore.variations.length > 0;
    return (
      <li key={move.place.join()}>
        <span className={step}>
          {numbered && (
            <span className={moveNumber}>
              {`${move.number}${move.side === "white" ? "." : "..."}`}
            </span>
          )}
          <button
            type="button"
            className={moveText}
            aria-current={
              move.place.join() === currentPlace ? "true" : undefined
            }
            onClick={() => onGoTo(move.place)}
          >
            {move.text}
          </button>
        </span>
        {move.variations.map((variationMoves) => (
          <span key={variationMoves[0]?.place.join()} className={variation}>
            (
            <ol className={line}>
              <Moves
                moves={variationMoves}
                currentPlace={currentPlace}
                onGoTo={onGoTo}
              />
            </ol>
            )
          </span>
        ))}
      </li>
    );
  });
}
