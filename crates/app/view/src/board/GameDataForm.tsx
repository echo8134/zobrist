import { ChevronDown } from "lucide-react";
import { useId, useLayoutEffect, useState } from "react";
import type { EditableTag } from "../generated/EditableTag";
import type { GamePicture } from "../generated/GamePicture";
import type { GameResult } from "../generated/GameResult";
import styles from "./GameDataForm.module.css";
import { isSaveKey, type Tab, tagValue } from "./tabs";

const { form, field, fieldName, control, refused, hint, choice, actions } =
  styles;

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
  mode: Tab["gameData"];
  picture: GamePicture;
  /** Sends the text of a field and tells whether Rust took it. */
  onSetTag: (tag: EditableTag, text: string) => Promise<boolean>;
  onSetResult: (result: GameResult) => void;
  onSave: () => void;
  onCancel: () => void;
};

export function GameDataForm({
  mode,
  picture,
  onSetTag,
  onSetResult,
  onSave,
  onCancel,
}: Props) {
  const resultId = useId();
  useLayoutEffect(() => {
    if (mode !== "openToSave") {
      return;
    }
    const act = (event: KeyboardEvent) => {
      const target = event.target;
      const inField =
        target instanceof HTMLElement &&
        (target.matches("input, textarea, select") || target.isContentEditable);
      if (event.key === "Escape") {
        event.preventDefault();
        if (inField) {
          target.blur();
        }
        onCancel();
      } else if (event.key === "Enter" && target === document.body) {
        event.preventDefault();
        onSave();
      }
    };
    // Escape must also reach the form while a field keeps other keys to itself.
    document.addEventListener("keydown", act, true);
    return () => document.removeEventListener("keydown", act, true);
  }, [mode, onSave, onCancel]);

  if (mode === "closed") {
    return null;
  }

  return (
    <form
      className={form}
      aria-label="Game data"
      onSubmit={(event) => event.preventDefault()}
      // A key that is pressed in the form belongs to the form. It does not reach the keys
      // that walk through the game or change it. Only the key of Save goes on to the tab.
      onKeyDown={(event) => {
        if (!isSaveKey(event)) {
          event.stopPropagation();
        }
      }}
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
      {mode === "openToSave" && (
        <div className={actions}>
          <button className="actionButton" type="button" onClick={onSave}>
            Save
          </button>
          <button className="actionButton" type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      )}
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
          // A save takes the text of the field with it.
          if (event.key === "Enter" || isSaveKey(event)) {
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
