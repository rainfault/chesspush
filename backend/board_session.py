"""Legal board state, independent of Qt and Chessground's presentation."""
from io import StringIO
import chess
import chess.pgn


class BoardSession:
    def __init__(self):
        self.root = chess.STARTING_FEN
        self.moves = []
        self.cursor = 0
        self.revision = 0
        self.orientation = 'white'
        self.annotations = {}

    @property
    def board(self):
        board = chess.Board(self.root)
        for move in self.moves[:self.cursor]:
            board.push(move)
        return board

    def play(self, origin, destination, promotion='', revision=None):
        if revision is not None and revision != self.revision:
            raise ValueError('Позиция уже изменилась')
        board = self.board
        # Lichess allows castling by dropping the king on its rook.
        piece = board.piece_at(chess.parse_square(origin))
        if piece and piece.piece_type == chess.KING and origin[0] == 'e':
            rook = board.piece_at(chess.parse_square(destination))
            if rook == chess.Piece(chess.ROOK, piece.color) and origin[1] == destination[1]:
                destination = ('g' if destination[0] == 'h' else 'c') + origin[1]
        move = chess.Move.from_uci(origin + destination + promotion)
        if move not in board.legal_moves:
            raise ValueError('Недопустимый ход')
        self.moves[self.cursor:] = [move]
        self.annotations = {i: shapes for i, shapes in self.annotations.items() if i <= self.cursor}
        self.cursor += 1
        self.revision += 1

    def seek(self, cursor):
        self.cursor = max(0, min(len(self.moves), int(cursor)))
        self.revision += 1

    def load_pgn(self, text):
        if not text.strip():
            self.load_fen(chess.STARTING_FEN)
            return
        game = chess.pgn.read_game(StringIO(text))
        if game is None or game.errors:
            raise ValueError('Некорректный PGN')
        moves = list(game.mainline_moves())
        if not moves and not text.lstrip().startswith('[') and text.strip() != '*':
            raise ValueError('В PGN не найдены ходы')
        root = game.board()
        if type(root) is not chess.Board or root.chess960 or not root.is_valid():
            raise ValueError('Поддерживаются стандартные шахматы')
        self.root = root.fen()
        self.moves = moves
        self.cursor = len(moves)
        self.annotations = {}
        self.revision += 1

    def load_fen(self, fen):
        board = chess.Board(fen)
        if not board.is_valid():
            raise ValueError('Некорректная позиция FEN')
        self.root, self.moves, self.cursor = board.fen(), [], 0
        self.annotations = {}
        self.revision += 1

    def set_shapes(self, shapes, revision):
        if revision != self.revision:
            return
        valid = []
        for shape in shapes[:64]:
            if shape.get('orig') not in chess.SQUARE_NAMES:
                continue
            if shape.get('dest') and shape['dest'] not in chess.SQUARE_NAMES:
                continue
            if shape.get('brush') not in ('green', 'red', 'blue', 'yellow'):
                continue
            valid.append({key: shape[key] for key in ('orig', 'dest', 'brush') if key in shape})
        self.annotations[self.cursor] = valid

    def snapshot(self):
        board = self.board
        dests = {}
        promotions = []
        for move in board.legal_moves:
            origin, dest = chess.square_name(move.from_square), chess.square_name(move.to_square)
            targets = dests.setdefault(origin, [])
            if dest not in targets:
                targets.append(dest)
            if move.promotion:
                promotions.append(origin + dest)
            if board.is_castling(move):
                targets.append(('h' if move.to_square > move.from_square else 'a') + origin[1])
        replay = chess.Board(self.root)
        rows = []
        for index, move in enumerate(self.moves):
            prefix = f'{replay.fullmove_number}.' if replay.turn else (f'{replay.fullmove_number}…' if index == 0 else '')
            rows.append({'ply': index + 1, 'text': prefix + replay.san(move)})
            replay.push(move)
        pgn = chess.pgn.Game.from_board(board).accept(chess.pgn.StringExporter(headers=False, comments=False, variations=False))
        pgn = pgn.rsplit(' ', 1)[0] if ' ' in pgn else ''
        return dict(fen=board.fen(), pgn=pgn, dests=dests, promotions=list(set(promotions)),
                    turn='white' if board.turn else 'black', check=board.is_check(),
                    lastMove=[chess.square_name(board.peek().from_square), chess.square_name(board.peek().to_square)] if board.move_stack else None,
                    orientation=self.orientation, cursor=self.cursor, total=len(self.moves), moves=rows,
                    shapes=self.annotations.get(self.cursor, []), revision=self.revision,
                    standardRoot=self.root == chess.STARTING_FEN)
