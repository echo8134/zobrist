import type { NotationMove } from "./generated/NotationMove";
import styles from "./Notation.module.css";

const { notation, entry, moveNumber, moveText } = styles;

type Props = {
  mainLine: readonly NotationMove[];
  currentPlace: readonly number[];
};

export function Notation({ mainLine, currentPlace }: Props) {
  const current = currentPlace.join();
  return (
    <ol className={notation}>
      {mainLine.map((move) => (
        <li key={move.place.join()} className={entry}>
          {move.side === "white" && (
            <span className={moveNumber}>{move.number}.</span>
          )}
          <span
            className={moveText}
            aria-current={move.place.join() === current ? "true" : undefined}
          >
            {move.text}
          </span>
        </li>
      ))}
    </ol>
  );
}
