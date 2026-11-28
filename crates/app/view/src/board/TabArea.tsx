import { ArrowUpDown } from "lucide-react";
import { useEffect, useLayoutEffect } from "react";
import { AnnotationPanel } from "../AnnotationPanel";
import { IconButton } from "../controls/IconButton";
import type { EditableTag } from "../generated/EditableTag";
import type { GameResult } from "../generated/GameResult";
import { Notation } from "../Notation";
import { Tablebase } from "../Tablebase";
import { Board } from "./Board";
import { GameDataForm } from "./GameDataForm";
import { NotationTools } from "./NotationTools";
import { PositionPanel, PositionTools } from "./PositionTools";
import type { SetupEdit } from "./positionSetup";
import styles from "./TabArea.module.css";
import {
  type Annotation,
  type Change,
  isSaveKey,
  type Move,
  type NotationTool,
  placeToAskFor,
  refusedSaveLine,
  type SaveTool,
  type Shapes,
  type Tab,
  toolForKey,
} from "./tabs";

const {
  area,
  boardPart,
  boardStage,
  boardColumn,
  buttonRow,
  notationArea,
  refusedSave,
  recordedValues,
  notationScroller,
} = styles;

type Props = {
  tab: Tab;
  change: (change: Change) => void;
  onEditSetup: (edit: SetupEdit) => void;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onGoTo: (place: number[]) => void;
  onTool: (tool: NotationTool) => void;
  onSave: (tool: SaveTool) => void;
  onSetGameData: (gameData: Tab["gameData"]) => void;
  onAnnotate: (annotation: Annotation) => void;
  onShapes: (shapes: Shapes) => void;
  onSetTag: (tag: EditableTag, text: string) => Promise<boolean>;
  onSetResult: (result: GameResult) => void;
};

export function TabArea({
  tab,
  change,
  onEditSetup,
  onTurnBoard,
  onMove,
  onGoTo,
  onTool,
  onSave,
  onSetGameData,
  onAnnotate,
  onShapes,
  onSetTag,
  onSetResult,
}: Props) {
  const { panel, picture } = tab;
  const saveMessage = tab.refusedSave
    ? refusedSaveLine(tab.refusedSave)
    : tab.saveWarning;
  // A layout effect, so that a key right after a new picture is drawn starts from that picture.
  // A plain effect runs later, and until then the key would start from the picture before.
  useLayoutEffect(() => {
    if (panel !== undefined) return;
    const act = (event: KeyboardEvent) => {
      // The key of Save works in a field too. The field sends its text first.
      if (isSaveKey(event)) {
        event.preventDefault();
        onSave("Save");
        return;
      }
      const { target } = event;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable)
      ) {
        return;
      }
      const tool = toolForKey(event);
      const place = placeToAskFor(event.key, picture);
      if (tool !== undefined) {
        event.preventDefault();
        onTool(tool);
      } else if (place !== undefined) {
        event.preventDefault();
        onGoTo(place);
      }
    };
    document.addEventListener("keydown", act);
    return () => document.removeEventListener("keydown", act);
  }, [panel, picture, onGoTo, onTool, onSave]);

  // The board keeps focus on a mouse press. A button can also receive a click without a
  // mouse press. In both cases the field must send its text before the action.
  useEffect(() => {
    const leaveField = ({ target }: MouseEvent) => {
      const focused = document.activeElement;
      if (
        (focused instanceof HTMLInputElement ||
          focused instanceof HTMLTextAreaElement ||
          focused instanceof HTMLSelectElement) &&
        focused !== target
      ) {
        focused.blur();
      }
    };
    document.addEventListener("mousedown", leaveField, true);
    document.addEventListener("click", leaveField, true);
    return () => {
      document.removeEventListener("mousedown", leaveField, true);
      document.removeEventListener("click", leaveField, true);
    };
  }, []);

  return (
    <main className={area}>
      <div className={boardPart}>
        <div className={boardStage}>
          <div className={boardColumn}>
            <Board
              key={panel?.type === "setup" ? "setup" : "game"}
              inert={panel?.type !== "paste" && panel?.status === "pending"}
              setup={
                panel?.type === "setup"
                  ? {
                      draft: panel.draft,
                      onPieces: (pieces) =>
                        onEditSetup({ type: "Fields", fields: { pieces } }),
                    }
                  : undefined
              }
              picture={picture}
              bottomSide={tab.bottomSide}
              onMove={onMove}
              onShapes={onShapes}
            />
            <div className={buttonRow}>
              <IconButton
                name="Turn the board"
                icon={ArrowUpDown}
                onClick={onTurnBoard}
              />
              <PositionTools tab={tab} change={change} />
            </div>
          </div>
        </div>
        {panel === undefined && (
          <Tablebase key={picture.revision} game={tab.game} picture={picture} />
        )}
      </div>
      <PositionPanel
        onEditSetup={onEditSetup}
        tab={tab}
        change={change}
        className={notationArea}
        annotations={
          <AnnotationPanel picture={picture} onAnnotate={onAnnotate} />
        }
      >
        <NotationTools
          picture={picture}
          onUse={onTool}
          onSave={onSave}
          gameDataOpen={tab.gameData !== "closed"}
          onToggleGameData={() =>
            onSetGameData(tab.gameData === "closed" ? "open" : "closed")
          }
        />
        {saveMessage && (
          <p className={refusedSave} role="alert">
            {saveMessage}
          </p>
        )}
        <GameDataForm
          mode={tab.gameData}
          picture={picture}
          onSetTag={onSetTag}
          onSetResult={onSetResult}
          onSave={() => onSave("ConfirmSave")}
          onCancel={() => onSetGameData("closed")}
        />
        {(picture.evaluation !== undefined || picture.clock !== undefined) && (
          <ul className={recordedValues} aria-label="Recorded move values">
            {picture.evaluation !== undefined && (
              <li>
                Evaluation <strong>{picture.evaluation}</strong>
              </li>
            )}
            {picture.clock !== undefined && (
              <li>
                {picture.clock.side === "white" ? "White clock" : "Black clock"}{" "}
                <strong>{picture.clock.time}</strong>
              </li>
            )}
          </ul>
        )}
        <div className={notationScroller}>
          <Notation
            mainLine={picture.notation}
            startComments={picture.startComments}
            currentPlace={picture.place}
            onGoTo={onGoTo}
          />
        </div>
      </PositionPanel>
    </main>
  );
}
