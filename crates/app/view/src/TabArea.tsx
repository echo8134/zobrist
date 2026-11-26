import { ArrowUpDown } from "lucide-react";
import { useEffect } from "react";
import { Board } from "./Board";
import { IconButton } from "./IconButton";
import { Notation } from "./Notation";
import styles from "./TabArea.module.css";
import { type Move, placeToAskFor, type Tab } from "./tabs";

const { area, boardPart, boardColumn, buttonRow, notationArea } = styles;

type Props = {
  tab: Tab;
  onTurnBoard: () => void;
  onMove: (move: Move) => void;
  onGoTo: (place: number[]) => void;
};

export function TabArea({ tab, onTurnBoard, onMove, onGoTo }: Props) {
  const { picture } = tab;
  useEffect(() => {
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
          mainLine={picture.notation}
          currentPlace={picture.place}
          onGoTo={onGoTo}
        />
      </aside>
    </main>
  );
}
