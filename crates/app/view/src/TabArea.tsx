import { ArrowUpDown } from "lucide-react";
import { useLayoutEffect } from "react";
import { Board } from "./Board";
import { GameDataForm } from "./GameDataForm";
import type { EditableTag } from "./generated/EditableTag";
import type { GameResult } from "./generated/GameResult";
import { IconButton } from "./IconButton";
import { Notation } from "./Notation";
import styles from "./TabArea.module.css";
import { type Move, placeToAskFor, type Shapes, type Tab } from "./tabs";

const { area, boardPart, boardColumn, buttonRow, notationArea, notationPart } =
  styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onGoTo: (place: number[]) => void;
  onShapes: (shapes: Shapes) => void;
  onSetTag: (tag: EditableTag, text: string) => Promise<boolean>;
  onSetResult: (result: GameResult) => void;
};

export function TabArea({
  tab,
  onTurnBoard,
  onMove,
  onGoTo,
  onShapes,
  onSetTag,
  onSetResult,
}: Props) {
  const { picture } = tab;
  // A layout effect, so that a key right after a new picture is drawn starts from that picture.
  // A plain effect runs later, and until then the key would start from the picture before.
  useLayoutEffect(() => {
    const walk = (event: KeyboardEvent) => {
      const { target } = event;
      const takesText =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable);
      const place = takesText ? undefined : placeToAskFor(event.key, picture);
      if (place !== undefined) {
        event.preventDefault();
        onGoTo(place);
      }
    };
    document.addEventListener("keydown", walk);
    return () => document.removeEventListener("keydown", walk);
  }, [picture, onGoTo]);

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
        <GameDataForm
          picture={picture}
          onSetTag={onSetTag}
          onSetResult={onSetResult}
        />
        <div className={notationPart}>
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
