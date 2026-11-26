import { moveSymbolNames } from "./AnnotationPanel";
import type { NotationMove } from "./generated/NotationMove";
import styles from "./Notation.module.css";

const { notation, line, step, moveNumber, moveText, comment, variation } =
  styles;

type Props = {
  mainLine: readonly NotationMove[];
  startComments: readonly string[];
  currentPlace: readonly number[];
  onGoTo: (place: number[]) => void;
};

export function Notation({
  mainLine,
  startComments,
  currentPlace,
  onGoTo,
}: Props) {
  return (
    <ol className={notation}>
      {startComments.length > 0 && (
        <li>
          <Comments texts={startComments} />
        </li>
      )}
      <Moves
        moves={mainLine}
        currentPlace={currentPlace.join()}
        onGoTo={onGoTo}
      />
    </ol>
  );
}

function Comments({ texts }: { texts: readonly string[] }) {
  return texts.length > 0 && <span className={comment}>{texts.join(" ")}</span>;
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
      moveBefore.variations.length > 0 ||
      moveBefore.commentsAfter.length > 0 ||
      move.commentsBefore.length > 0;
    const symbols = move.symbols
      .map((symbol) => (symbol in moveSymbolNames ? symbol : ` ${symbol}`))
      .join("");
    return (
      <li key={move.place.join()}>
        <Comments texts={move.commentsBefore} />
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
            {`${move.text}${symbols}`}
          </button>
        </span>
        <Comments texts={move.commentsAfter} />
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
