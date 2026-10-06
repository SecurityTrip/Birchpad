"""Tests of coverage_floors.py: python3 -m unittest discover -s .github -p "*_test.py"."""

import unittest

from coverage_floors import check, crate_of, lines_by_crate


def report(*files):
    return {
        "data": [
            {
                "files": [
                    {"filename": name, "summary": {"lines": {"covered": covered, "count": count}}}
                    for name, covered, count in files
                ]
            }
        ]
    }


class CrateOf(unittest.TestCase):
    def test_a_file_belongs_to_the_crate_folder_it_is_in(self):
        self.assertEqual(crate_of("/home/runner/work/Birchpad/crates/core/src/lib.rs"), "core")
        self.assertEqual(crate_of(r"G:\Projects\notepad\crates\io\src\decode.rs"), "io")

    def test_the_innermost_crates_folder_counts(self):
        self.assertEqual(crate_of("/srv/crates/birchpad/crates/view/src/a.rs"), "view")

    def test_files_outside_crates_belong_to_none(self):
        self.assertIsNone(crate_of("/home/runner/work/Birchpad/fuzz/fuzz_targets/a.rs"))
        self.assertIsNone(crate_of("crates"))
        self.assertIsNone(crate_of(""))


class Check(unittest.TestCase):
    def test_lines_add_up_per_crate(self):
        totals = lines_by_crate(
            report(
                ("/r/crates/core/src/a.rs", 8, 10),
                ("/r/crates/core/src/b.rs", 2, 10),
                ("/r/crates/io/src/c.rs", 0, 0),
                ("/r/other/d.rs", 5, 5),
            )
        )
        self.assertEqual(totals, {"core": (10, 20), "io": (0, 0)})

    def test_crates_at_or_over_their_floors_pass(self):
        table, problems = check({"core": (90, 100), "io": (0, 0)}, {"core": 90, "io": 100})
        self.assertEqual(problems, [])
        self.assertIn("| core | 100 | 90.00 % | 90 % |", table)
        self.assertIn("| io | 0 | 100.00 % | 100 % |", table)

    def test_a_crate_under_its_floor_or_without_one_fails(self):
        table, problems = check({"core": (8999, 10000), "view": (1, 1)}, {"core": 90})
        self.assertIn("| core | 10000 | 89.99 % | 90 % (under) |", table)
        self.assertIn("| view | 1 | 100.00 % | none |", table)
        self.assertEqual(
            problems,
            [
                "core: 89.99 % of lines, under its floor of 90 %",
                "view has no floor in coverage-floors.toml",
            ],
        )

    def test_a_floor_for_a_crate_that_is_gone_fails(self):
        _, problems = check({}, {"old": 50})
        self.assertEqual(problems, ["old has a floor but no code was measured"])

    def test_a_crate_well_over_its_floor_asks_for_a_higher_one(self):
        table, problems = check({"core": (95, 100), "io": (929, 1000)}, {"core": 92, "io": 90})
        self.assertEqual(problems, [])
        self.assertIn("| core | 100 | 95.00 % | 92 % (raise to 95) |", table)
        self.assertIn("| io | 1000 | 92.90 % | 90 % |", table)


if __name__ == "__main__":
    unittest.main()
