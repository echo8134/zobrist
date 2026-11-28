import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { Config } from "@lichess-org/chessground/config";
import type { DrawShape } from "@lichess-org/chessground/draw";
import { initial, write } from "@lichess-org/chessground/fen";
import type { Key } from "@lichess-org/chessground/types";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.brown.css";
import "./pieces.css";
import { useLayoutEffect, useRef, useState } from "react";
import type { GamePicture } from "../generated/GamePicture";
import type { MoveSquares } from "../generated/MoveSquares";
import type { PositionFields } from "../generated/PositionFields";
import type { ShapeColor } from "../generated/ShapeColor";
import type { Side } from "../generated/Side";
import styles from "./Board.module.css";
import { PromotionChooser } from "./PromotionChooser";
import type { PositionSetup } from "./positionSetup";
import type { Move, Shapes } from "./tabs";

const { board, ground } = styles;

export function presetFields(preset: "start" | "empty"): PositionFields {
  const start = preset === "start";
  return {
    pieces: start ? initial : write(new Map()),
    turn: "white",
    whiteShort: start,
    whiteLong: start,
    blackShort: start,
    blackLong: start,
    enPassant: null,
  };
}

type Props = {
  picture: GamePicture;
  bottomSide: Side;
  inert: boolean;
  onMove: (move: Move) => void;
  setup?:
    | {
        draft: PositionSetup;
        onPieces: (pieces: string) => void;
      }
    | undefined;
  /** Gets all shapes of the board after the player drew one or took some away. */
  onShapes: (shapes: Shapes) => void;
};

/** The board component draws with these four colors, and they are the colors of the picture. */
function isShapeColor(brush: string | undefined): brush is ShapeColor {
  return (
    brush === "green" ||
    brush === "red" ||
    brush === "blue" ||
    brush === "yellow"
  );
}

/** A shape of the board component without a target square is a circle on its square. */
function shapesOfBoard(drawn: readonly DrawShape[]): Shapes {
  const shapes: Shapes = { squares: [], arrows: [] };
  for (const { orig, dest, brush } of drawn) {
    if (!isShapeColor(brush)) {
      continue;
    }
    if (dest === undefined) {
      shapes.squares.push({ color: brush, square: orig });
    } else {
      shapes.arrows.push({ color: brush, from: orig, to: dest });
    }
  }
  return shapes;
}

export function Board({
  picture,
  bottomSide,
  inert,
  onMove,
  onShapes,
  setup,
}: Props) {
  const groundElement = useRef<HTMLDivElement>(null);
  const groundApi = useRef<Api>(null);
  const [promotionInPicture, setPromotionInPicture] = useState<{
    picture: GamePicture;
    squares: MoveSquares;
  } | null>(null);
  const waitingPromotion =
    setup === undefined && promotionInPicture?.picture === picture
      ? promotionInPicture.squares
      : null;

  useLayoutEffect(() => {
    if (groundElement.current === null) {
      return;
    }
    let drawn: Config;
    if (setup === undefined) {
      const targets = new Map<Key, Key[]>();
      for (const { from, to } of picture.legalMoves) {
        targets.set(from as Key, [
          ...(targets.get(from as Key) ?? []),
          to as Key,
        ]);
      }
      const takesMoves =
        waitingPromotion === null && picture.legalMoves.length > 0;
      drawn = {
        autoCastle: true,
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
                setPromotionInPicture({ picture, squares: { from, to } });
              } else {
                onMove({ from, to, promotion: null });
              }
            },
          },
        },
        drawable: {
          enabled: true,
          // The shapes belong to their place. Without this, the click that starts a move would
          // take them away from the place that the move leaves.
          eraseOnMovablePieceClick: false,
          // The board keeps what the player drew only until the next picture, so it never shows
          // a shape that Rust does not have at this place.
          shapes: [
            ...picture.squares.map(({ color, square }) => ({
              orig: square as Key,
              brush: color,
            })),
            ...picture.arrows.map(({ color, from, to }) => ({
              orig: from as Key,
              dest: to as Key,
              brush: color,
            })),
          ],
          onChange: (drawnShapes) => onShapes(shapesOfBoard(drawnShapes)),
        },
      };
    } else {
      const { position } = setup.draft;
      drawn = {
        orientation: bottomSide,
        ...(groundApi.current && setup.draft.pending
          ? {}
          : { fen: position.fields.pieces }),
        turnColor: position.fields.turn,
        autoCastle: false,
        lastMove: [],
        check: false,
        selectable: { enabled: false },
        draggable: { enabled: true },
        movable: {
          free: true,
          color: "both",
          dests: new Map(),
          events: {
            after: () => {
              const pieces = groundApi.current?.getFen();
              if (pieces !== undefined) {
                setup.onPieces(pieces);
              }
            },
          },
        },
        drawable: { enabled: false, shapes: [] },
      };
    }
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
    const tool = setup?.draft.tool;
    if (!setup || !tool) {
      return;
    }

    const element = groundElement.current;
    const api = groundApi.current;
    let pressed: Key | undefined;
    const squareAt = (event: MouseEvent | TouchEvent) => {
      const point =
        "changedTouches" in event
          ? event.changedTouches.item(0)
          : event.button === 0
            ? event
            : null;
      return point
        ? api.getKeyAtDomPos([point.clientX, point.clientY])
        : undefined;
    };
    const press = (event: MouseEvent | TouchEvent) => {
      pressed = squareAt(event);
    };
    const release = (event: MouseEvent | TouchEvent) => {
      const square = squareAt(event);
      // A drag moves the piece that was on the board, even with a palette tool chosen.
      if (square !== undefined && square === pressed) {
        api.setPieces(
          new Map([[square, tool === "eraser" ? undefined : tool]]),
        );
        setup.onPieces(api.getFen());
      }
      pressed = undefined;
    };
    element.addEventListener("mousedown", press);
    element.addEventListener("mouseup", release);
    element.addEventListener("touchstart", press);
    element.addEventListener("touchend", release);
    return () => {
      element.removeEventListener("mousedown", press);
      element.removeEventListener("mouseup", release);
      element.removeEventListener("touchstart", press);
      element.removeEventListener("touchend", release);
    };
  }, [picture, bottomSide, waitingPromotion, onMove, onShapes, setup]);

  useLayoutEffect(
    () => () => {
      groundApi.current?.destroy();
      groundApi.current = null;
    },
    [],
  );

  return (
    <div className={board} inert={inert}>
      <div ref={groundElement} className={ground} />
      {waitingPromotion !== null && (
        <PromotionChooser
          side={picture.turn}
          targetSquare={waitingPromotion.to}
          bottomSide={bottomSide}
          onChoose={(promotion) => {
            setPromotionInPicture(null);
            onMove({ ...waitingPromotion, promotion });
          }}
          onCancel={() => setPromotionInPicture(null)}
        />
      )}
    </div>
  );
}
