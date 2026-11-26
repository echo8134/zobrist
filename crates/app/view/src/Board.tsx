import { Chessground } from "@lichess-org/chessground";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.brown.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";
import { useLayoutEffect, useRef } from "react";
import styles from "./Board.module.css";
import type { Side } from "./generated/Side";

const { board } = styles;

type Props = {
  fen: string;
  bottomSide: Side;
};

export function Board({ fen, bottomSide }: Props) {
  const wrap = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (wrap.current === null) {
      return;
    }
    const ground = Chessground(wrap.current, {
      fen,
      orientation: bottomSide,
      viewOnly: true,
    });
    return () => ground.destroy();
  }, [fen, bottomSide]);
  return <div ref={wrap} className={board} />;
}
