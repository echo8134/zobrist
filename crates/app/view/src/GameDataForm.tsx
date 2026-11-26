import { ChevronDown } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import styles from "./GameDataForm.module.css";
import type { EditableTag } from "./generated/EditableTag";
import type { GamePicture } from "./generated/GamePicture";
import type { GameResult } from "./generated/GameResult";
import { tagValue } from "./tabs";

const { form, field, fieldName, control, refused, hint, choice } = styles;

/** The fields in the order of the form, two in a row. */
const fields = ["White", "Black", "Event", "Site", "Date", "Round"] as const;

const results: readonly { result: GameResult; text: string }[] = [
  { result: "unknown", text: "*" },
  { result: "whiteWin", text: "1-0" },
  { result: "blackWin", text: "0-1" },
  { result: "draw", text: "1/2-1/2" },
];

const refusedDate =
  "Write the date as 2026.11.26. Use ?? for a part that is unknown.";
const refusedText = "This text cannot be written into a game file.";

type Props = {
  picture: GamePicture;
  /** Sends the text of a field and tells whether Rust took it. */
  onSetTag: (tag: EditableTag, text: string) => Promise<boolean>;
  onSetResult: (result: GameResult) => void;
};

export function GameDataForm({ picture, onSetTag, onSetResult }: Props) {
  const formElement = useRef<HTMLFormElement>(null);
  const resultId = useId();

  // The board keeps the window from moving the focus when the player clicks on it. So a
  // press of the mouse outside the form takes the focus from the form here, and the field
  // that had it sends its text.
  useEffect(() => {
    const leaveForm = ({ target }: MouseEvent) => {
      const focused = document.activeElement;
      const inForm = (node: unknown) =>
        node instanceof Node && formElement.current?.contains(node) === true;
      if (
        focused instanceof HTMLElement &&
        inForm(focused) &&
        !inForm(target)
      ) {
        focused.blur();
      }
    };
    document.addEventListener("mousedown", leaveForm, true);
    return () => document.removeEventListener("mousedown", leaveForm, true);
  }, []);

  return (
    <form
      ref={formElement}
      className={form}
      aria-label="Game data"
      onSubmit={(event) => event.preventDefault()}
      // A key that is pressed in the form belongs to the form. It does not reach the keys
      // that walk through the game or change it.
      onKeyDown={(event) => event.stopPropagation()}
    >
      {fields.map((name) => (
        <Field
          key={name}
          name={name}
          value={tagValue(picture, name)}
          onSend={(text) => onSetTag(name, text)}
        />
      ))}
      <div className={field}>
        <label className={fieldName} htmlFor={resultId}>
          Result
        </label>
        <div className={choice}>
          <select
            id={resultId}
            className={control}
            value={picture.result}
            onChange={(event) => {
              const chosen = results.find(
                ({ result }) => result === event.target.value,
              );
              if (chosen !== undefined) {
                onSetResult(chosen.result);
              }
            }}
          >
            {results.map(({ result, text }) => (
              <option key={result} value={result}>
                {text}
              </option>
            ))}
          </select>
          <ChevronDown aria-hidden="true" />
        </div>
      </div>
    </form>
  );
}

type FieldProps = {
  name: EditableTag;
  /** The value of the tag in the picture. */
  value: string;
  onSend: (text: string) => Promise<boolean>;
};

function Field({ name, value, onSend }: FieldProps) {
  const id = useId();
  const hintId = useId();
  const [typed, setTyped] = useState(value);
  // The text that Rust refused. The field keeps it and marks itself, and this is the one
  // text of the form that the picture does not have.
  const [refusedTyped, setRefusedTyped] = useState<string>();
  const [sent, setSent] = useState<string>();
  const [valueSeen, setValueSeen] = useState(value);
  if (valueSeen !== value) {
    // The field starts again from the picture when its tag changes there. A text that the
    // player typed on while Rust answered the text before stays, because no picture has it.
    setValueSeen(value);
    const text = typed.trim();
    if (text === valueSeen || text === sent) {
      setTyped(value);
    }
    setRefusedTyped(undefined);
  }

  const send = async () => {
    // Rust takes white space at the ends away, so it makes no difference here either.
    const text = typed.trim();
    if (text === value) {
      setTyped(value);
      setRefusedTyped(undefined);
    } else if (text !== refusedTyped) {
      setSent(text);
      const taken = await onSend(text);
      setRefusedTyped(taken ? undefined : text);
    }
  };

  const isRefused = refusedTyped !== undefined;
  return (
    <div className={field}>
      <label className={fieldName} htmlFor={id}>
        {name}
      </label>
      <input
        id={id}
        className={isRefused ? `${control} ${refused}` : control}
        type="text"
        autoComplete="off"
        spellCheck={false}
        value={typed}
        aria-invalid={isRefused}
        aria-describedby={isRefused ? hintId : undefined}
        onChange={(event) => setTyped(event.target.value)}
        onBlur={() => void send()}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            void send();
          }
        }}
      />
      {isRefused && (
        <span id={hintId} className={hint}>
          {name === "Date" ? refusedDate : refusedText}
        </span>
      )}
    </div>
  );
}
