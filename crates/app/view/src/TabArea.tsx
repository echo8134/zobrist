import { ArrowUpDown } from "lucide-react";
import { Board } from "./Board";
import { IconButton } from "./IconButton";
import styles from "./TabArea.module.css";
import type { Tab } from "./tabs";

const { area, boardPart, boardColumn, buttonRow, notationArea } = styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
};

export function TabArea({ tab, onTurnBoard }: Props) {
  return (
    <main className={area}>
      <div className={boardPart}>
        <div className={boardColumn}>
          <Board fen={tab.picture.fen} bottomSide={tab.bottomSide} />
          <div className={buttonRow}>
            <IconButton
              name="Turn the board"
              icon={ArrowUpDown}
              onClick={onTurnBoard}
            />
          </div>
        </div>
      </div>
      <aside className={notationArea} />
    </main>
  );
}
