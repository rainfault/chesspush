from pathlib import Path
import unittest

from backend.search.opening_catalog import builtin_opening_choices, opening_book, scan_database_openings


class OpeningCatalogTest(unittest.TestCase):
    def test_builtin_choices_include_practical_openings(self):
        labels = {choice["label"] for choice in builtin_opening_choices()}
        self.assertIn("Any opening", labels)
        self.assertTrue(any(label.startswith("London System") for label in labels))
        self.assertTrue(any(label.startswith("Caro-Kann Defense") for label in labels))

    def test_opening_book_search_variants_and_line_match(self):
        book = opening_book()
        families = book.search_families("caro kann")
        self.assertEqual(families[0]["family"], "Caro-Kann Defense")
        variants = book.variants_for_family(families[0]["key"], eco="B12")
        self.assertTrue(any("Advance" in variant["name"] for variant in variants))
        matches = book.match_line("1. e4 c6 2. d4 d5")
        self.assertTrue(any(match["eco"].startswith("B") for match in matches))

    def test_scan_database_openings_from_sample_pgn(self):
        path = Path(__file__).resolve().parents[1] / "sample_data" / "sample_games.pgn"
        choices = scan_database_openings(path, max_games=10)
        labels = [choice["label"] for choice in choices]
        self.assertTrue(any("Caro-Kann" in label for label in labels))
        self.assertTrue(any("London" in label for label in labels))


if __name__ == "__main__":
    unittest.main()
