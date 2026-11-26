import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { Config } from "@lichess-org/chessground/config";
import type { Key } from "@lichess-org/chessground/types";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.brown.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";
import { useLayoutEffect, useRef, useState } from "react";
import styles from "./Board.module.css";
import type { GamePicture } from "./generated/GamePicture";
import type { MoveSquares } from "./generated/MoveSquares";
import type { Side } from "./generated/Side";
import { PromotionChooser } from "./PromotionChooser";
import type { Move } from "./tabs";

const { board, ground } = styles;

type Props = {
  picture: GamePicture;
  bottomSide: Side;
  onMove: (move: Move) => void;
};

export function Board({ picture, bottomSide, onMove }: Props) {
  const groundElement = useRef<HTMLDivElement>(null);
  const groundApi = useRef<Api>(null);
  const [waitingPromotion, setWaitingPromotion] = useState<MoveSquares | null>(
    null,
  );

  useLayoutEffect(() => {
    if (groundElement.current === null) {
      return;
    }
    const targets = new Map<Key, Key[]>();
    for (const { from, to } of picture.legalMoves) {
      targets.set(from as Key, [
        ...(targets.get(from as Key) ?? []),
        to as Key,
      ]);
    }
    const takesMoves =
      waitingPromotion === null && picture.legalMoves.length > 0;
    const drawn: Config = {
      fen: picture.fen,
      orientation: bottomSide,
      turnColor: picture.turn,
      check: picture.inCheck,
      lastMove:
        picture.lastMove === null
          ? []
          : [picture.lastMove.from as Key, picture.lastMove.to as Key],
      selectable: { enabled: takesMoves },
      draggable: { enabled: takesMoves },
      movable: {
        free: false,
        color: picture.turn,
        dests: targets,
        events: {
          after: (from, to) => {
            const played = picture.legalMoves.find(
              (legal) => legal.from === from && legal.to === to,
            );
            if (played?.promotion) {
              setWaitingPromotion({ from, to });
            } else {
              onMove({ from, to, promotion: null });
            }
          },
        },
      },
    };
    if (groundApi.current === null) {
      // The first picture goes in when the board is made. Set afterwards, its pieces would
      // glide there from the start position. Without `trustAllEvents` the board component
      // takes only the events of a real mouse, and the driver of the end-to-end test makes
      // its events in the page.
      groundApi.current = Chessground(groundElement.current, {
        ...drawn,
        trustAllEvents: true,
      });
    } else {
      groundApi.current.set(drawn);
    }
  }, [picture, bottomSide, waitingPromotion, onMove]);

  useLayoutEffect(
    () => () => {
      groundApi.current?.destroy();
      groundApi.current = null;
    },
    [],
  );

  return (
    <div className={board}>
      <div ref={groundElement} className={ground} />
      {waitingPromotion !== null && (
        <PromotionChooser
          side={picture.turn}
          targetSquare={waitingPromotion.to}
          bottomSide={bottomSide}
          onChoose={(promotion) => {
            setWaitingPromotion(null);
            onMove({ ...waitingPromotion, promotion });
          }}
          onCancel={() => setWaitingPromotion(null)}
        />
      )}
    </div>
  );
}
