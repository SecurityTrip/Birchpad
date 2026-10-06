"""Line coverage per crate, against the floors in .github/coverage-floors.toml.

Reads `cargo llvm-cov report --json --summary-only` on stdin, prints a Markdown table of each
crate's lines (for the run's summary) and fails if a crate is under its floor, or has no floor.
Floors only rise: when a crate is well over its floor, the table says so, and the floor is
raised in the same change that added the tests.

    cargo llvm-cov report --json --summary-only | python3 .github/coverage_floors.py
"""

import json
import sys
import tomllib
from pathlib import Path

FLOORS = Path(__file__).with_name("coverage-floors.toml")
# How far over its floor a crate may be before the table asks for the floor to be raised. Floors
# sit a little under the numbers measured, since the platforms run different code.
SLACK = 3.0


def crate_of(filename):
    """The crate directory a source file belongs to, or None for files outside crates/."""
    parts = filename.replace("\\", "/").split("/")
    for index in range(len(parts) - 2, -1, -1):
        if parts[index] == "crates":
            return parts[index + 1]
    return None


def lines_by_crate(report):
    totals = {}
    for data in report["data"]:
        for file in data["files"]:
            crate = crate_of(file["filename"])
            if crate is None:
                continue
            lines = file["summary"]["lines"]
            covered, count = totals.get(crate, (0, 0))
            totals[crate] = (covered + lines["covered"], count + lines["count"])
    return totals


def check(totals, floors):
    """The Markdown table and the problems."""
    rows = ["| Crate | Lines | Covered | Floor |", "|---|---:|---:|---:|"]
    problems = []
    for crate in sorted(totals):
        covered, count = totals[crate]
        percent = 100.0 * covered / count if count else 100.0
        floor = floors.get(crate)
        note = ""
        if floor is None:
            problems.append(f"{crate} has no floor in {FLOORS.name}")
        elif percent < floor:
            problems.append(f"{crate}: {percent:.2f} % of lines, under its floor of {floor} %")
            note = " (under)"
        elif percent >= floor + SLACK:
            note = f" (raise to {int(percent)})"
        shown = "none" if floor is None else f"{floor} %"
        rows.append(f"| {crate} | {count} | {percent:.2f} % | {shown}{note} |")
    for crate in sorted(set(floors) - set(totals)):
        problems.append(f"{crate} has a floor but no code was measured")
    return "\n".join(rows), problems


def main():
    totals = lines_by_crate(json.load(sys.stdin))
    floors = tomllib.loads(FLOORS.read_text(encoding="utf-8"))["lines"]
    table, problems = check(totals, floors)
    print(table)
    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
