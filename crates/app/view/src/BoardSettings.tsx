import { createContext, type ReactNode, use, useRef, useState } from "react";
import styles from "./BoardSettings.module.css";
import type { BoardAppearance } from "./generated/BoardAppearance";
import { ask } from "./ipc";

const { settings: panel, group, row, refusal } = styles;

type Settings = { appearance: BoardAppearance | null; error: string | null };

const initial = ask({ type: "GetBoardAppearance" }).then<Settings, Settings>(
  ({ appearance }) => ({ appearance, error: null }),
  (error: unknown) => ({
    appearance: null,
    error: error instanceof Error ? error.message : String(error),
  }),
);

const BoardSettings = createContext<
  | (Settings & {
      selection: BoardAppearance | null;
      save: (choice: BoardAppearance) => void;
    })
  | null
>(null);

export function BoardSettingsProvider({ children }: { children: ReactNode }) {
  const loaded = use(initial);
  const [settings, setSettings] = useState(loaded);
  const [selection, setSelection] = useState(loaded.appearance);
  const lastSave = useRef(initial);

  function save(choice: BoardAppearance) {
    setSelection(choice);
    // Each request writes a complete choice. Keep their order even when the player
    // changes the next control before the previous save has returned.
    const writing = lastSave.current.then(
      async (previous): Promise<Settings> => {
        try {
          const { appearance } = await ask({
            type: "SetBoardAppearance",
            appearance: choice,
          });
          return { appearance, error: null };
        } catch (error) {
          return {
            appearance: previous.appearance,
            error: `Could not save board settings. ${String(error)}`,
          };
        }
      },
    );
    lastSave.current = writing;
    void writing.then((saved) => {
      setSettings(saved);
      if (lastSave.current === writing) setSelection(saved.appearance);
    });
  }

  return (
    <BoardSettings value={{ ...settings, selection, save }}>
      <div
        data-piece-set={settings.appearance?.pieceSet ?? "cburnett"}
        data-board-palette={settings.appearance?.boardPalette}
        data-show-coordinates={settings.appearance?.showCoordinates}
      >
        {children}
      </div>
    </BoardSettings>
  );
}

export function BoardSettingsForm() {
  const settings = use(BoardSettings);
  if (!settings) throw new Error("Board settings need a window.");
  const { selection: appearance, save, error } = settings;

  return (
    <section className={panel} aria-label="Board appearance">
      {appearance && (
        <fieldset className={group}>
          <legend>Board appearance</legend>
          <label className={row}>
            Piece set
            <select
              value={appearance.pieceSet}
              onChange={({ currentTarget: { value } }) => {
                if (value === "cburnett" || value === "chessnut")
                  save({ ...appearance, pieceSet: value });
              }}
            >
              <option value="cburnett">Cburnett</option>
              <option value="chessnut">Chessnut</option>
            </select>
          </label>
          <label className={row}>
            Board colours
            <select
              value={appearance.boardPalette}
              onChange={({ currentTarget: { value } }) => {
                if (value === "brown" || value === "green")
                  save({ ...appearance, boardPalette: value });
              }}
            >
              <option value="brown">Brown</option>
              <option value="green">Green</option>
            </select>
          </label>
          <label className={row}>
            Show coordinates
            <input
              type="checkbox"
              checked={appearance.showCoordinates}
              onChange={({ currentTarget: { checked } }) =>
                save({ ...appearance, showCoordinates: checked })
              }
            />
          </label>
        </fieldset>
      )}
      {error && (
        <p role="alert" className={refusal}>
          {error}
        </p>
      )}
    </section>
  );
}
