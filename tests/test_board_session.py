import unittest
import chess
from backend.board_session import BoardSession


class BoardSessionTest(unittest.TestCase):
    def test_legal_moves_and_rejected_stale_command(self):
        session = BoardSession()
        session.play('e2', 'e4', revision=0)
        self.assertEqual(session.snapshot()['pgn'], '1. e4')
        before = session.snapshot()
        with self.assertRaises(ValueError): session.play('e7', 'e5', revision=0)
        with self.assertRaises(ValueError): session.play('e7', 'e4', revision=1)
        self.assertEqual(session.snapshot(), before)

    def test_en_passant_and_rook_square_castling(self):
        session = BoardSession()
        session.load_pgn('1. e4 a6 2. e5 d5')
        session.play('e5', 'd6')
        self.assertIsNone(session.board.piece_at(chess.D5))
        session.load_pgn('1. e4 e5 2. Nf3 Nc6 3. Bc4 Nf6')
        session.play('e1', 'h1')
        self.assertEqual(session.board.piece_at(chess.F1), chess.Piece(chess.ROOK, chess.WHITE))
        self.assertEqual(session.board.king(chess.WHITE), chess.G1)

    def test_underpromotion_is_required_and_supported(self):
        session = BoardSession()
        session.load_fen('7k/P7/8/8/8/8/8/7K w - - 0 1')
        self.assertIn('a7a8', session.snapshot()['promotions'])
        with self.assertRaises(ValueError): session.play('a7', 'a8')
        session.play('a7', 'a8', 'n')
        self.assertEqual(session.board.piece_at(chess.A8).piece_type, chess.KNIGHT)

    def test_navigation_preserves_annotations_and_new_line_discards_future(self):
        session = BoardSession()
        session.load_pgn('1. e4 e5 2. Nf3')
        session.set_shapes([dict(orig='f3', dest='e5', brush='green')], session.revision)
        session.seek(1)
        self.assertEqual(session.snapshot()['shapes'], [])
        session.seek(3)
        self.assertEqual(len(session.snapshot()['shapes']), 1)
        session.seek(1)
        session.play('c7', 'c5')
        self.assertEqual(session.snapshot()['pgn'], '1. e4 c5')
        self.assertNotIn(3, session.annotations)

    def test_bad_pgn_does_not_replace_position(self):
        session = BoardSession()
        session.load_pgn('1. d4 d5')
        before = session.snapshot()
        with self.assertLogs('chess.pgn', level='ERROR'):
            with self.assertRaises(ValueError): session.load_pgn('1. e4 e5 2. Ke3')
        self.assertEqual(session.snapshot(), before)

    def test_shape_validation_and_stale_updates(self):
        session = BoardSession()
        session.set_shapes([dict(orig='e4', brush='green'), dict(orig='z9', brush='red')], 0)
        self.assertEqual(len(session.snapshot()['shapes']), 1)
        session.play('e2', 'e4')
        session.set_shapes([dict(orig='e5', brush='red')], 0)
        self.assertEqual(session.snapshot()['shapes'], [])

    def test_imported_fen_is_distinguished_from_opening_prefix(self):
        session = BoardSession()
        session.load_pgn('[SetUp "1"]\n[FEN "7k/P7/8/8/8/8/8/7K w - - 0 1"]\n\n1. a8=Q+ *')
        self.assertFalse(session.snapshot()['standardRoot'])
        session.seek(0)
        self.assertEqual(session.board.piece_at(chess.A7).piece_type, chess.PAWN)
