import { type ReactNode, useEffect, useState } from "react";
import { answersOnTheirWay, ask } from "./ipc";
import { PositionSetupPanel } from "./PositionSetupPanel";
import styles from "./PositionTools.module.css";
import type { SetupEdit } from "./positionSetup";
import type { Change, PositionPanelState, Tab } from "./tabs";

const {
  actions,
  openingProblem,
  pastePanel,
  pgnField,
  problem,
  setupArea,
  textButton,
} = styles;

type Props = {
  tab: Tab;
  change: (change: Change) => void;
};

export function PositionTools({
  tab: { game, panel, picture },
  change,
}: Props) {
  const [copied, setCopied] = useState<{ format: "FEN" | "PGN" }>();
  const [exportError, setExportError] = useState<{
    picture: Tab["picture"];
    reason: string;
  }>();

  useEffect(() => {
    if (copied === undefined) return;
    const timer = window.setTimeout(() => setCopied(undefined), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  async function pgnText() {
    await answersOnTheirWay(game);
    const answer = await ask({ type: "GamePgn", game });
    if ("error" in answer) {
      setExportError({ picture, reason: answer.error });
      return null;
    }
    setExportError(undefined);
    return answer.text;
  }

  async function copy(format: "FEN" | "PGN") {
    const text = format === "FEN" ? picture.fen : pgnText();
    const contents = Promise.resolve(text).then((value) => {
      if (value === null) throw new Error("There is no PGN to copy.");
      return new Blob([value], { type: "text/plain" });
    });
    // Start the write during the click. Its contents can arrive after the gesture expires.
    try {
      await navigator.clipboard.write([
        new ClipboardItem({ "text/plain": contents }),
      ]);
    } catch (error) {
      if ((await text) !== null) throw error;
      return;
    }
    setCopied({ format });
  }

  const disabled = panel !== undefined;
  return (
    <>
      <button
        type="button"
        className={textButton}
        disabled={disabled}
        onClick={async () => {
          change({
            type: "setPanel",
            game,
            panel: { type: "openingSetup", status: "pending" },
          });
          const { draft } = await ask({
            type: "DescribePosition",
            input: { type: "Fen", fen: picture.fen },
          });
          change({
            type: "setPanel",
            game,
            panel:
              draft === null
                ? undefined
                : {
                    type: "setup",
                    draft: { position: draft, raw: null, tool: null },
                  },
          });
        }}
      >
        Set up position
      </button>
      <button
        type="button"
        className={textButton}
        disabled={disabled}
        onClick={() => copy("FEN")}
      >
        {copied?.format === "FEN" ? "Copied" : "Copy FEN"}
      </button>
      <button
        type="button"
        className={textButton}
        disabled={disabled}
        onClick={() => copy("PGN")}
      >
        {copied?.format === "PGN" ? "Copied" : "Copy PGN"}
      </button>
      {exportError?.picture === picture && (
        <p className={problem} role="alert">
          This game cannot be written as PGN. {exportError.reason}
        </p>
      )}
      <button
        type="button"
        className={textButton}
        disabled={disabled}
        onClick={() =>
          change({ type: "setPanel", game, panel: { type: "paste", text: "" } })
        }
      >
        Paste PGN
      </button>
    </>
  );
}

export function PositionPanel({
  tab,
  change,
  className,
  children,
  onEditSetup,
}: Props & {
  className: string | undefined;
  children: ReactNode;
  onEditSetup: (edit: SetupEdit) => void;
}) {
  const { game, panel } = tab;
  const setPanel = (next: PositionPanelState | undefined) =>
    change({ type: "setPanel", game, panel: next });
  let controls: ReactNode;
  if (panel?.type === "setup") {
    controls = (
      <PositionSetupPanel
        setup={panel.draft}
        refused={panel.status === "refused"}
        onChoose={(tool) =>
          setPanel({ ...panel, draft: { ...panel.draft, tool } })
        }
        onEdit={onEditSetup}
        onCancel={() => setPanel(undefined)}
        onUse={async () => {
          setPanel({ ...panel, status: "pending" });
          const description = await (panel.draft.pending ?? panel.draft);
          const { change: answer } = await ask({
            type: "SetStartPosition",
            game,
            fen: description.position.fen,
          });
          if (answer?.refused) {
            setPanel({
              type: "setup",
              draft: { ...description, tool: panel.draft.tool },
              status: "refused",
            });
          } else if (answer !== null) {
            change({ type: "drawPicture", game, picture: answer.picture });
            setPanel(undefined);
          }
        }}
      />
    );
  } else if (panel?.type === "paste") {
    controls = (
      <div className={pastePanel}>
        <label>
          PGN text
          <textarea
            className={pgnField}
            value={panel.text}
            spellCheck={false}
            onChange={({ currentTarget }) =>
              setPanel({ type: "paste", text: currentTarget.value })
            }
          />
        </label>
        {panel.status === "refused" && (
          <p className={problem} role="alert">
            This text holds no game.
          </p>
        )}
        <div className={actions}>
          <button
            type="button"
            className={textButton}
            onClick={async () => {
              setPanel({ ...panel, status: "pending" });
              const { opened } = await ask({
                type: "OpenPgnText",
                text: panel.text,
              });
              if (opened === null) {
                setPanel({ ...panel, status: "refused" });
              } else {
                setPanel(undefined);
                change({
                  type: "openTab",
                  game: opened.game,
                  picture: opened.picture,
                  openingProblem: opened.problem,
                });
              }
            }}
          >
            Open as new game
          </button>
          <button
            type="button"
            className={textButton}
            onClick={() => setPanel(undefined)}
          >
            Cancel
          </button>
        </div>
      </div>
    );
  } else {
    controls = (
      <>
        {children}
        {tab.openingProblem && (
          <p className={openingProblem} role="alert">
            {tab.openingProblem}
          </p>
        )}
      </>
    );
  }
  return (
    <aside
      className={
        panel?.type === "setup" ? `${className} ${setupArea}` : className
      }
      inert={panel?.status === "pending"}
    >
      {controls}
    </aside>
  );
}
