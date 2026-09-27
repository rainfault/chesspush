import unittest

from backend.statistics.pain_index import calc_pain_index, priority_from_pain


class PainIndexTest(unittest.TestCase):
    def test_pain_index_positive_for_underperformance(self):
        self.assertGreater(calc_pain_index(20, 0.25), 0)

    def test_pain_index_zero_when_score_good(self):
        self.assertEqual(calc_pain_index(20, 0.60), 0)

    def test_priority(self):
        self.assertEqual(priority_from_pain(0), "норма")


if __name__ == "__main__":
    unittest.main()
