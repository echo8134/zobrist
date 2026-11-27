import { useLayoutEffect, useRef } from "react";
import styles from "./AnnotationPanel.module.css";
import type { GamePicture } from "./generated/GamePicture";
import type { MoveSymbol } from "./generated/MoveSymbol";
import type { PositionSymbol } from "./generated/PositionSymbol";
import { type Annotation, commentOfCurrentPlace } from "./tabs";

const { annotationPanel, commentField, symbolRow, symbolButton } = styles;

export const moveSymbolNames: Record<MoveSymbol, string> = {
  "!": "Good move",
  "?": "Mistake",
  "!!": "Brilliant move",
  "??": "Blunder",
  "!?": "Interesting move",
  "?!": "Dubious move",
};
const positionSymbolNames: Record<PositionSymbol, string> = {
  "=": "Equal position",
  "∞": "Unclear position",
  "⩲": "White is slightly better",
  "⩱": "Black is slightly better",
  "±": "White is better",
  "∓": "Black is better",
  "+-": "White is winning",
  "-+": "Black is winning",
};

type RowProps<Symbol extends string> = {
  symbols: Record<Symbol, string>;
  setSymbol: Symbol | null;
  disabled: boolean;
  onSet: (symbol: Symbol | null) => void;
};

function SymbolRow<Symbol extends string>({
  symbols,
  setSymbol,
  disabled,
  onSet,
}: RowProps<Symbol>) {
  return (
    <div className={symbolRow}>
      {(Object.keys(symbols) as Symbol[]).map((symbol) => (
        <button
          key={symbol}
          type="button"
          className={symbolButton}
          aria-label={symbols[symbol]}
          aria-pressed={symbol === setSymbol}
          disabled={disabled}
          onClick={() => onSet(symbol === setSymbol ? null : symbol)}
        >
          {symbol}
        </button>
      ))}
    </div>
  );
}

type Props = {
  picture: GamePicture;
  onAnnotate: (annotation: Annotation) => void;
};

export function AnnotationPanel({ picture, onAnnotate }: Props) {
  const field = useRef<HTMLTextAreaElement>(null);
  const commentInField = useRef("");
  const sentText = useRef<string>(undefined);
  useLayoutEffect(() => {
    const comment = commentOfCurrentPlace(picture);
    // The field takes the comment of a new picture while it holds the comment of the picture
    // before or the text that it sent. A text that the player typed on while Rust answered
    // stays, because no picture has it.
    if (
      field.current !== null &&
      [commentInField.current, sentText.current].includes(field.current.value)
    ) {
      field.current.value = comment;
    }
    commentInField.current = comment;
    sentText.current = undefined;
  }, [picture]);
  const sendComment = (text: string) => {
    if (text !== commentOfCurrentPlace(picture)) {
      sentText.current = text;
      onAnnotate({ type: "SetComment", text });
    }
  };
  const atStart = picture.place.length === 0;

  return (
    <div className={annotationPanel}>
      <textarea
        ref={field}
        className={commentField}
        aria-label="Comment"
        onBlur={(event) => sendComment(event.currentTarget.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && event.ctrlKey) {
            event.preventDefault();
            sendComment(event.currentTarget.value);
          }
        }}
      />
      <SymbolRow
        symbols={moveSymbolNames}
        setSymbol={picture.moveSymbol}
        disabled={atStart}
        onSet={(symbol) => onAnnotate({ type: "SetMoveSymbol", symbol })}
      />
      <SymbolRow
        symbols={positionSymbolNames}
        setSymbol={picture.positionSymbol}
        disabled={atStart}
        onSet={(symbol) => onAnnotate({ type: "SetPositionSymbol", symbol })}
      />
    </div>
  );
}
