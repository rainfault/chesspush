import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Chessground } from "@lichess-org/chessground";
import type { Api } from "@lichess-org/chessground/api";
import type { Config } from "@lichess-org/chessground/config";
import type { Role, SquareName } from "chessops/types";
import { BoardSession } from "./BoardSession";
import "@lichess-org/chessground/assets/chessground.base.css";
import "@lichess-org/chessground/assets/chessground.brown.css";
import "@lichess-org/chessground/assets/chessground.cburnett.css";
import "./board.css";

interface Props {
  session: BoardSession;
  revision: number;
  onChange: () => void;
  readOnly?: boolean;
}
export function ChessBoard({
  session,
  revision,
  onChange,
  readOnly = false,
}: Props) {
  const element = useRef<HTMLDivElement>(null);
  const api = useRef<Api | null>(null);
  const current = useRef({ session, onChange, readOnly });
  current.current = { session, onChange, readOnly };
  const [promotion, setPromotion] = useState<{
    orig: SquareName;
    dest: SquareName;
    revision: number;
  } | null>(null);

  function sync(force = false) {
    const ground = api.current,
      s = current.current.session;
    if (!ground) return;
    const config: Config = {
      orientation: s.orientation,
      turnColor: s.position.turn,
      check: s.position.isCheck(),
      lastMove: s.lastMove,
      movable: {
        free: false,
        color: current.current.readOnly ? undefined : s.position.turn,
        dests: current.current.readOnly ? new Map() : s.destinations,
      },
      draggable: { enabled: !current.current.readOnly },
      drawable: { shapes: s.shapes },
    };
    // Preserve the reference fix: do not restart an already painted move animation.
    if (force || ground.getFen() !== s.fen.split(" ")[0]) config.fen = s.fen;
    ground.set(config);
  }
  useLayoutEffect(() => {
    const node = element.current!;
    const ground = Chessground(node, {
      fen: session.fen,
      orientation: session.orientation,
      coordinates: true,
      ranksPosition: "right",
      disableContextMenu: true,
      animation: { enabled: true, duration: 200 },
      draggable: { enabled: !readOnly, showGhost: true },
      selectable: { enabled: true },
      premovable: { enabled: false },
      movable: {
        free: false,
        color: readOnly ? undefined : session.position.turn,
        dests: readOnly ? new Map() : session.destinations,
        events: {
          after: (origin, destination) => {
            if (
              !/^[a-h][1-8]$/.test(origin) ||
              !/^[a-h][1-8]$/.test(destination)
            )
              return;
            const orig = origin as SquareName,
              dest = destination as SquareName;
            const { session: s, onChange: changed } = current.current;
            if (s.promotionRequired(orig, dest)) {
              ground.set({ movable: { dests: new Map() } });
              setPromotion({ orig, dest, revision: s.revision });
              return;
            }
            if (s.play(orig, dest)) changed();
            else sync(true);
          },
        },
      },
      drawable: {
        enabled: true,
        visible: true,
        onChange: (shapes) => {
          current.current.session.setShapes(shapes);
        },
      },
    });
    api.current = ground;
    const observer = new ResizeObserver(() => ground.redrawAll());
    observer.observe(node);
    return () => {
      observer.disconnect();
      ground.destroy();
      api.current = null;
    };
  }, []);
  useLayoutEffect(() => {
    sync();
    if (promotion && promotion.revision !== revision) setPromotion(null);
  }, [revision, readOnly]);
  function cancelPromotion() {
    setPromotion(null);
    sync(true);
  }
  useEffect(() => {
    if (!promotion) return;
    const escape = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancelPromotion();
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [promotion]);
  return (
    <div className="board-frame">
      <div className="cg-wrap" ref={element} aria-label="Шахматная доска" />
      {promotion && (
        <div
          className="promotion"
          role="dialog"
          aria-label="Превращение пешки"
          onClick={cancelPromotion}
        >
          <div
            className="promotion-choices cg-wrap"
            style={{
              left: `${(session.orientation === "white" ? promotion.dest.charCodeAt(0) - 97 : 104 - promotion.dest.charCodeAt(0)) * 12.5}%`,
              top:
                session.position.turn === session.orientation ? "0" : undefined,
              bottom:
                session.position.turn !== session.orientation ? "0" : undefined,
              flexDirection:
                session.position.turn === session.orientation
                  ? "column"
                  : "column-reverse",
            }}
          >
            {(["queen", "knight", "rook", "bishop"] as Role[]).map((role) => (
              <button
                key={role}
                aria-label={role}
                ref={(node) => {
                  if (node) {
                    const piece = document.createElement("piece");
                    piece.className = `${session.position.turn} ${role}`;
                    node.replaceChildren(piece);
                  }
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  if (
                    session.play(
                      promotion.orig,
                      promotion.dest,
                      role,
                      promotion.revision,
                    )
                  )
                    onChange();
                  setPromotion(null);
                  sync(true);
                }}
              ></button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
