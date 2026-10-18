"""The share of mutants a crate's tests catch, against its floor in .github/mutants-floors.toml.

Reads cargo-mutants' `mutants.out/outcomes.json` on stdin, for the crate named as the argument
(its folder in crates/), prints a Markdown line for the run's summary and fails if the share is
under the floor or the crate has no floor. Mutants that time out count as caught (a test noticed
that something was wrong); those that do not build count for nothing.

    python3 .github/mutants_floors.py core < mutants.out/outcomes.json
"""

import json
import sys
import tomllib
from pathlib import Path

FLOORS = Path(__file__).with_name("mutants-floors.toml")
# How far over its floor a crate may be before the line asks for the floor to be raised.
SLACK = 3.0


def share(outcomes):
    """The caught, missed and total of the mutants that built, and the share caught in percent."""
    caught = outcomes.get("caught", 0) + outcomes.get("timeout", 0)
    missed = outcomes.get("missed", 0)
    total = caught + missed
    return caught, missed, (100.0 * caught / total if total else 100.0)


def check(crate, outcomes, floors):
    """The Markdown line and the problems."""
    caught, missed, percent = share(outcomes)
    floor = floors.get(crate)
    problems = []
    note = ""
    if floor is None:
        problems.append(f"{crate} has no floor in {FLOORS.name}")
    elif percent < floor:
        problems.append(f"{crate}: {percent:.1f} % of mutants caught, under its floor of {floor} %")
        note = " (under)"
    elif percent >= floor + SLACK:
        note = f" (raise to {int(percent)})"
    shown = "none" if floor is None else f"{floor} %"
    line = f"| {crate} | {caught} | {missed} | {percent:.1f} % | {shown}{note} |"
    return line, problems


def main():
    crate = sys.argv[1]
    outcomes = json.load(sys.stdin)
    floors = tomllib.loads(FLOORS.read_text(encoding="utf-8"))["caught"]
    line, problems = check(crate, outcomes, floors)
    print("| Crate | Caught | Missed | Share | Floor |")
    print("|---|---:|---:|---:|---:|")
    print(line)
    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
