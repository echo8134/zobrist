import { ArrowUpDown } from "lucide-react";
import { Board } from "./Board";
import { IconButton } from "./IconButton";
import { Notation } from "./Notation";
import styles from "./TabArea.module.css";
import type { Move, Shapes, Tab } from "./tabs";

const { area, boardPart, boardColumn, buttonRow, notationArea } = styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onShapes: (shapes: Shapes) => void;
};

export function TabArea({ tab, onTurnBoard, onMove, onShapes }: Props) {
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
        <Notation
          mainLine={tab.picture.notation}
          currentPlace={tab.picture.place}
        />
      </aside>
    </main>
  );
}
