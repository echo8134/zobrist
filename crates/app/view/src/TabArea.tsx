import { ArrowUpDown } from "lucide-react";
import { useLayoutEffect } from "react";
import { Board } from "./Board";
import { GameDataForm } from "./GameDataForm";
import type { EditableTag } from "./generated/EditableTag";
import type { GameResult } from "./generated/GameResult";
import { IconButton } from "./IconButton";
import { Notation } from "./Notation";
import { NotationTools } from "./NotationTools";
import styles from "./TabArea.module.css";
import {
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
  boardColumn,
  buttonRow,
  notationArea,
  refusedSave,
  notationScroller,
} = styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onGoTo: (place: number[]) => void;
  onTool: (tool: NotationTool) => void;
  onSave: (tool: SaveTool) => void;
  onShapes: (shapes: Shapes) => void;
  onSetTag: (tag: EditableTag, text: string) => Promise<boolean>;
  onSetResult: (result: GameResult) => void;
};

export function TabArea({
  tab,
  onTurnBoard,
  onMove,
  onGoTo,
  onTool,
  onSave,
  onShapes,
  onSetTag,
  onSetResult,
}: Props) {
  const { picture } = tab;
  // A layout effect, so that a key right after a new picture is drawn starts from that picture.
  // A plain effect runs later, and until then the key would start from the picture before.
  useLayoutEffect(() => {
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
  }, [picture, onGoTo, onTool, onSave]);

  return (
    <main className={area}>
      <div className={boardPart}>
        <div className={boardColumn}>
          <Board
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
          </div>
        </div>
      </div>
      <aside className={notationArea}>
        <NotationTools picture={picture} onUse={onTool} onSave={onSave} />
        {tab.refusedSave !== undefined && (
          <p className={refusedSave} role="alert">
            {refusedSaveLine(tab.refusedSave)}
          </p>
        )}
        <GameDataForm
          picture={picture}
          onSetTag={onSetTag}
          onSetResult={onSetResult}
        />
        <div className={notationScroller}>
          <Notation
            mainLine={picture.notation}
            currentPlace={picture.place}
            onGoTo={onGoTo}
          />
        </div>
      </aside>
    </main>
  );
}
