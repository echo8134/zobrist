import { Eraser } from "lucide-react";
import { createElement } from "react";
import styles from "./PositionSetupPanel.module.css";
import toolStyles from "./PositionTools.module.css";
import {
  changePosition,
  editFen,
  emptyFen,
  fenOf,
  type PositionSetup,
  roles,
  startingFen,
} from "./positionSetup";

const {
  panel,
  palette,
  pieceButton,
  pieceImage,
  group,
  options,
  option,
  field,
  control,
  buttons,
  buttonGroup,
  refusal,
} = styles;
const { textButton } = toolStyles;
const sides = ["white", "black"] as const;
const sideNames = { white: "White", black: "Black" };
const rights = [
  ["K", "White short"],
  ["Q", "White long"],
  ["k", "Black short"],
  ["q", "Black long"],
] as const;

type Props = {
  setup: PositionSetup;
  onChange: (setup: PositionSetup) => void;
  onUse: (fen: string) => void;
  onCancel: () => void;
  refused: boolean;
};

export function PositionSetupPanel({
  setup,
  onChange,
  onUse,
  onCancel,
  refused,
}: Props) {
  const { position, invalid, tool } = setup;
  const fen = invalid?.field === "fen" ? invalid.text : fenOf(position);
  const enPassant =
    invalid?.field === "enPassant"
      ? invalid.text
      : position.enPassant === "-"
        ? ""
        : position.enPassant;
  return (
    <div className={panel}>
      <fieldset className={`cg-wrap ${palette}`} aria-label="Pieces">
        {sides.flatMap((color) =>
          roles.map((role) => (
            <button
              key={`${color} ${role}`}
              type="button"
              className={pieceButton}
              aria-label={`${sideNames[color]} ${role}`}
              aria-pressed={
                tool !== "eraser" && tool?.color === color && tool.role === role
              }
              onClick={() => onChange({ ...setup, tool: { color, role } })}
            >
              {createElement("piece", {
                className: `${pieceImage} ${color} ${role}`,
              })}
            </button>
          )),
        )}
        <button
          type="button"
          className={pieceButton}
          aria-label="Eraser"
          aria-pressed={tool === "eraser"}
          onClick={() => onChange({ ...setup, tool: "eraser" })}
        >
          <Eraser aria-hidden="true" />
        </button>
      </fieldset>
      <fieldset className={group}>
        <legend>Side to move</legend>
        <div className={options}>
          {sides.map((turn) => (
            <label key={turn} className={option}>
              <input
                type="radio"
                name="side-to-move"
                checked={position.turn === turn}
                onChange={() => onChange(changePosition(setup, { turn }))}
              />
              {sideNames[turn]}
            </label>
          ))}
        </div>
      </fieldset>
      <fieldset className={group}>
        <legend>Castling rights</legend>
        <div className={options}>
          {rights.map(([right, name]) => (
            <label key={right} className={option}>
              <input
                type="checkbox"
                checked={position.castling.includes(right)}
                onChange={({ currentTarget }) => {
                  const castling = rights
                    .filter(([letter]) =>
                      letter === right
                        ? currentTarget.checked
                        : position.castling.includes(letter),
                    )
                    .map(([letter]) => letter)
                    .join("");
                  onChange(
                    changePosition(setup, { castling: castling || "-" }),
                  );
                }}
              />
              {name}
            </label>
          ))}
        </div>
      </fieldset>
      <label className={field}>
        En passant square
        <input
          className={control}
          size={2}
          maxLength={2}
          value={enPassant}
          aria-invalid={invalid?.field === "enPassant"}
          onChange={({ currentTarget: { value } }) =>
            onChange(
              value === "" || /^[a-h][1-8]$/.test(value)
                ? changePosition(setup, { enPassant: value || "-" })
                : { ...setup, invalid: { field: "enPassant", text: value } },
            )
          }
        />
      </label>
      <label className={field}>
        FEN
        <input
          className={control}
          value={fen}
          aria-invalid={invalid?.field === "fen"}
          spellCheck={false}
          onChange={({ currentTarget: { value } }) =>
            onChange(editFen(setup, value))
          }
        />
      </label>
      {refused && (
        <p role="alert" className={refusal}>
          This is not a legal position.
        </p>
      )}
      <div className={buttons}>
        <div className={buttonGroup}>
          <button
            className={textButton}
            type="button"
            onClick={() => onChange(editFen(setup, startingFen))}
          >
            Start position
          </button>
          <button
            className={textButton}
            type="button"
            onClick={() => onChange(editFen(setup, emptyFen))}
          >
            Empty board
          </button>
        </div>
        <div className={buttonGroup}>
          <button
            className={textButton}
            type="button"
            disabled={invalid !== null}
            onClick={() => onUse(fen)}
          >
            Use position
          </button>
          <button className={textButton} type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
