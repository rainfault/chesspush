import { Chess, normalizeMove } from "chessops/chess";
import { parseFen, makeFen, INITIAL_FEN } from "chessops/fen";
import { parseSan, makeSan } from "chessops/san";
import { parseSquare, makeSquare } from "chessops/util";
import { chessgroundDests } from "chessops/compat";
import { parsePgn } from "chessops/pgn";
import type { Color, Move, Role, SquareName } from "chessops/types";
import type { DrawShape } from "@lichess-org/chessground/draw";

export class BoardSession {
  private positions: Chess[] = [Chess.default()];
  private moves: Move[] = [];
  private notes = new Map<number, DrawShape[]>();
  cursor = 0;
  orientation: Color = "white";
  revision = 0;
  get position() {
    return this.positions[this.cursor];
  }
  get fen() {
    return makeFen(this.position.toSetup());
  }
  get total() {
    return this.moves.length;
  }
  get standardRoot() {
    return makeFen(this.positions[0].toSetup()) === INITIAL_FEN;
  }
  get destinations() {
    return chessgroundDests(this.position);
  }
  get shapes() {
    return this.notes.get(this.cursor) ?? [];
  }
  get lastMove(): SquareName[] | undefined {
    const move = this.moves[this.cursor - 1];
    if (!move || !("from" in move)) return undefined;
    // Chessops normalizes castling to the rook square; highlight the king's destination.
    const previous = this.positions[this.cursor - 1];
    let to = move.to;
    if (
      previous.board.get(move.from)?.role === "king" &&
      previous.board.get(to)?.role === "rook"
    )
      to = (move.from & 56) + (to > move.from ? 6 : 2);
    return [makeSquare(move.from), makeSquare(to)];
  }
  get sanMoves() {
    return this.moves.map((move, i) => {
      const pos = this.positions[i];
      return `${pos.turn === "white" ? `${pos.fullmoves}. ` : i === 0 ? `${pos.fullmoves}... ` : ""}${makeSan(pos, move)}`;
    });
  }
  get pgn() {
    return this.sanMoves.slice(0, this.cursor).join(" ");
  }
  promotionRequired(orig: SquareName, dest: SquareName) {
    return (
      this.position.board.get(parseSquare(orig)!)?.role === "pawn" &&
      (dest[1] === "1" || dest[1] === "8")
    );
  }
  play(
    orig: SquareName,
    dest: SquareName,
    promotion?: Role,
    revision = this.revision,
  ) {
    if (revision !== this.revision) return false;
    const from = parseSquare(orig),
      to = parseSquare(dest);
    if (from === undefined || to === undefined) return false;
    const move = normalizeMove(this.position, { from, to, promotion });
    if (!this.position.isLegal(move)) return false;
    const next = this.position.clone();
    next.play(move);
    this.moves = [...this.moves.slice(0, this.cursor), move];
    this.positions = [...this.positions.slice(0, this.cursor + 1), next];
    for (const ply of this.notes.keys())
      if (ply > this.cursor) this.notes.delete(ply);
    this.cursor++;
    this.revision++;
    return true;
  }
  seek(ply: number) {
    this.cursor = Math.max(0, Math.min(this.total, ply));
    this.revision++;
  }
  flip() {
    this.orientation = this.orientation === "white" ? "black" : "white";
    this.revision++;
  }
  setShapes(shapes: DrawShape[], revision = this.revision) {
    if (revision !== this.revision) return;
    this.notes.set(
      this.cursor,
      shapes.slice(0, 64).map((s) => ({ ...s })),
    );
  }
  loadFen(fen: string) {
    const pos = Chess.fromSetup(parseFen(fen).unwrap()).unwrap();
    this.positions = [pos];
    this.moves = [];
    this.notes.clear();
    this.cursor = 0;
    this.revision++;
  }
  reset() {
    this.loadFen(INITIAL_FEN);
  }
  loadPgn(text: string) {
    if (!text.trim()) {
      this.reset();
      return;
    }
    const game = parsePgn(text)[0];
    if (!game) throw new Error("Некорректный PGN");
    const variant = game.headers.get("Variant");
    if (variant && !["Standard", "From Position"].includes(variant))
      throw new Error("Поддерживаются обычные шахматы");
    const root = Chess.fromSetup(
      parseFen(game.headers.get("FEN") ?? INITIAL_FEN).unwrap(),
    ).unwrap();
    const positions = [root];
    const moves: Move[] = [];
    // chessops' permissive PGN parser ignores invalid tokens. Validate the complete
    // mainline explicitly, so an invalid import cannot silently replace the board.
    let mainline = text
      .replace(/^\[.*\]\s*$/gm, "")
      .replace(/\{[^}]*\}/g, " ")
      .replace(/;[^\n]*/g, " ");
    while (/\([^()]*\)/.test(mainline))
      mainline = mainline.replace(/\([^()]*\)/g, " ");
    if (/[(){}]/.test(mainline))
      throw new Error("Незакрытый комментарий или вариант");
    mainline = mainline
      .replace(/\$\d+/g, " ")
      .replace(/\b\d+\.(?:\.\.)?/g, " ");
    for (const token of mainline.split(/\s+/).filter(Boolean)) {
      if (["*", "1-0", "0-1", "1/2-1/2"].includes(token)) continue;
      const pos = positions[positions.length - 1];
      const move = parseSan(pos, token.replace(/[!?]+$/g, ""));
      if (!move) throw new Error(`Некорректный ход: ${token}`);
      const next = pos.clone();
      next.play(move);
      positions.push(next);
      moves.push(move);
    }
    if (!moves.length && !text.trim().startsWith("["))
      throw new Error("PGN не содержит ходов");
    this.positions = positions;
    this.moves = moves;
    this.cursor = moves.length;
    this.notes.clear();
    this.revision++;
  }
}
