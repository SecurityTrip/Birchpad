import unittest

from mutants_floors import check, share


class ShareTest(unittest.TestCase):
    def test_timeouts_count_as_caught_and_unviable_ones_not_at_all(self):
        outcomes = {"caught": 6, "timeout": 2, "missed": 2, "unviable": 30}
        self.assertEqual(share(outcomes), (8, 2, 80.0))

    def test_no_mutants_is_everything_caught(self):
        self.assertEqual(share({}), (0, 0, 100.0))

    def test_all_missed(self):
        self.assertEqual(share({"missed": 4}), (0, 4, 0.0))


class CheckTest(unittest.TestCase):
    def test_at_the_floor_passes(self):
        line, problems = check("core", {"caught": 8, "missed": 2}, {"core": 80})
        self.assertEqual(problems, [])
        self.assertIn("80.0 %", line)

    def test_under_the_floor_fails(self):
        line, problems = check("core", {"caught": 7, "missed": 3}, {"core": 80})
        self.assertEqual(len(problems), 1)
        self.assertIn("(under)", line)

    def test_well_over_the_floor_asks_to_raise_it(self):
        line, problems = check("core", {"caught": 9, "missed": 1}, {"core": 80})
        self.assertEqual(problems, [])
        self.assertIn("(raise to 90)", line)

    def test_a_crate_without_a_floor_fails(self):
        _, problems = check("new", {"caught": 1}, {})
        self.assertEqual(problems, ["new has no floor in mutants-floors.toml"])


if __name__ == "__main__":
    unittest.main()
