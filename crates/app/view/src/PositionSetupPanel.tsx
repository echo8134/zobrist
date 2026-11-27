import { Eraser } from "lucide-react";
import { createElement } from "react";
import { presetFields } from "./Board";
import styles from "./PositionSetupPanel.module.css";
import toolStyles from "./PositionTools.module.css";
import { type PositionSetup, roles, type SetupEdit } from "./positionSetup";

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
  ["whiteShort", "White short"],
  ["whiteLong", "White long"],
  ["blackShort", "Black short"],
  ["blackLong", "Black long"],
] as const;

type Props = {
  setup: PositionSetup;
  onChoose: (tool: PositionSetup["tool"]) => void;
  onEdit: (edit: SetupEdit) => void;
  onUse: () => void;
  onCancel: () => void;
  refused: boolean;
};

export function PositionSetupPanel({
  setup,
  onChoose,
  onEdit,
  onUse,
  onCancel,
  refused,
}: Props) {
  const { position, raw, pending, tool } = setup;
  const fields = position.fields;
  const fen = raw?.field === "fen" ? raw.text : position.fen;
  const enPassant =
    raw?.field === "enPassant" ? raw.text : (fields.enPassant ?? "");
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
              onClick={() => onChoose({ color, role })}
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
          onClick={() => onChoose("eraser")}
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
                checked={fields.turn === turn}
                onChange={() => onEdit({ type: "Fields", fields: { turn } })}
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
                checked={fields[right]}
                onChange={({ currentTarget }) =>
                  onEdit({
                    type: "Fields",
                    fields: { [right]: currentTarget.checked },
                  })
                }
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
          value={enPassant}
          aria-invalid={pending === undefined && raw?.field === "enPassant"}
          onChange={({ currentTarget: { value } }) =>
            onEdit({ type: "Fields", fields: { enPassant: value || null } })
          }
        />
      </label>
      <label className={field}>
        FEN
        <input
          className={control}
          value={fen}
          aria-invalid={pending === undefined && raw?.field === "fen"}
          spellCheck={false}
          onChange={({ currentTarget: { value } }) =>
            onEdit({ type: "Fen", fen: value })
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
            onClick={() =>
              onEdit({ type: "Fields", fields: presetFields("start") })
            }
          >
            Start position
          </button>
          <button
            className={textButton}
            type="button"
            onClick={() =>
              onEdit({ type: "Fields", fields: presetFields("empty") })
            }
          >
            Empty board
          </button>
        </div>
        <div className={buttonGroup}>
          <button className={textButton} type="button" onClick={onUse}>
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
