import { ArrowUpDown } from "lucide-react";
import { Board } from "./Board";
import { GameDataForm } from "./GameDataForm";
import type { EditableTag } from "./generated/EditableTag";
import type { GameResult } from "./generated/GameResult";
import { IconButton } from "./IconButton";
import { Notation } from "./Notation";
import styles from "./TabArea.module.css";
import type { Move, Shapes, Tab } from "./tabs";

const { area, boardPart, boardColumn, buttonRow, notationArea, notationPart } =
  styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onShapes: (shapes: Shapes) => void;
  onSetTag: (tag: EditableTag, text: string) => Promise<boolean>;
  onSetResult: (result: GameResult) => void;
};

export function TabArea({
  tab,
  onTurnBoard,
  onMove,
  onShapes,
  onSetTag,
  onSetResult,
}: Props) {
  return (
    <main className={area}>
      <div className={boardPart}>
        <div className={boardColumn}>
          <Board
            picture={tab.picture}
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
          picture={tab.picture}
          onSetTag={onSetTag}
          onSetResult={onSetResult}
        />
        <div className={notationPart}>
          <Notation
            mainLine={tab.picture.notation}
            currentPlace={tab.picture.place}
          />
        </div>
      </aside>
    </main>
  );
}
