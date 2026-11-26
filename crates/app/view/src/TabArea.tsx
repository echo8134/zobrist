import { ArrowUpDown } from "lucide-react";
import { useLayoutEffect } from "react";
import { AnnotationPanel } from "./AnnotationPanel";
import { Board } from "./Board";
import { GameDataForm } from "./GameDataForm";
import type { EditableTag } from "./generated/EditableTag";
import type { GameResult } from "./generated/GameResult";
import { IconButton } from "./IconButton";
import { Notation } from "./Notation";
import { NotationTools } from "./NotationTools";
import styles from "./TabArea.module.css";
import {
  type Annotation,
  type Move,
  type NotationTool,
  placeToAskFor,
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
  notationScroller,
} = styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onGoTo: (place: number[]) => void;
  onTool: (tool: NotationTool) => void;
  onAnnotate: (annotation: Annotation) => void;
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
  onAnnotate,
  onShapes,
  onSetTag,
  onSetResult,
}: Props) {
  const { picture } = tab;
  // A layout effect, so that a key right after a new picture is drawn starts from that picture.
  // A plain effect runs later, and until then the key would start from the picture before.
  useLayoutEffect(() => {
    const act = (event: KeyboardEvent) => {
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
  }, [picture, onGoTo, onTool]);

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
        <NotationTools picture={picture} onUse={onTool} />
        <GameDataForm
          picture={picture}
          onSetTag={onSetTag}
          onSetResult={onSetResult}
        />
        <div className={notationScroller}>
          <Notation
            mainLine={picture.notation}
            startComments={picture.startComments}
            currentPlace={picture.place}
            onGoTo={onGoTo}
          />
        </div>
        <AnnotationPanel picture={picture} onAnnotate={onAnnotate} />
      </aside>
    </main>
  );
}
