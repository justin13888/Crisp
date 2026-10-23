#!/usr/bin/env python3
"""Per-package file-length gate.

Every authored file in a package must stay at or under that package's line
threshold (its p90 file length when the gate was introduced). Files that
already exceeded it are listed in a checked-in baseline with their line
count, and the baseline can only shrink. The check fails when:

- a file outside the baseline exceeds its package's threshold;
- a baselined file grows past the line count the baseline records for it;
- the baseline lists a file that no longer exceeds its threshold, or no
  longer exists (remove the entry).

Run from anywhere in the repository: ``just file-size``.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from collections.abc import Iterable, Mapping
from pathlib import Path

# Maximum lines per file, keyed by the package's top-level directory.
THRESHOLDS: dict[str, int] = {
    "core": 614,
    "server": 456,
    "wasm": 400,
    "client": 415,
}

# Generated output, which no one edits by hand.
EXCLUDED_PREFIXES: tuple[str, ...] = ("client/src/generated/",)

# Lockfiles, which a package manager writes.
EXCLUDED_NAMES: frozenset[str] = frozenset({"Cargo.lock", "bun.lock"})

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BASELINE = REPO_ROOT / "scripts" / "file-size-baseline.txt"


class BaselineError(ValueError):
    """The baseline file is malformed."""


def package_of(path: str) -> str | None:
    """Return the package a repository-relative path belongs to, if any."""
    top = path.split("/", 1)[0]
    return top if top in THRESHOLDS and "/" in path else None


def is_measured(path: str) -> bool:
    """Whether the gate measures this repository-relative path."""
    if package_of(path) is None:
        return False
    if path.startswith(EXCLUDED_PREFIXES):
        return False
    return path.rsplit("/", 1)[-1] not in EXCLUDED_NAMES


def count_lines(data: bytes) -> int | None:
    """Count lines like ``wc -l``, plus an unterminated last line.

    Returns None for binary content (any NUL byte), which is not measured.
    """
    if b"\0" in data:
        return None
    lines = data.count(b"\n")
    if data and not data.endswith(b"\n"):
        lines += 1
    return lines


def parse_baseline(text: str) -> dict[str, int]:
    """Parse ``<lines> <path>`` entries; ``#`` starts a comment line."""
    entries: dict[str, int] = {}
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(maxsplit=1)
        if len(parts) != 2 or not parts[0].isdigit():
            raise BaselineError(f"line {number}: expected '<lines> <path>', got {raw!r}")
        count, path = int(parts[0]), parts[1]
        if path in entries:
            raise BaselineError(f"line {number}: duplicate entry for {path}")
        entries[path] = count
    return entries


def evaluate(counts: Mapping[str, int], baseline: Mapping[str, int]) -> list[str]:
    """Return one message per violation; an empty list means the gate passes.

    ``counts`` maps every measured path to its line count.
    """
    problems: list[str] = []
    for path in sorted(counts):
        lines = counts[path]
        package = package_of(path)
        if package is None:
            continue
        limit = THRESHOLDS[package]
        if lines <= limit:
            continue
        recorded = baseline.get(path)
        if recorded is None:
            problems.append(
                f"{path}: {lines} lines exceeds the {package} threshold of {limit}; split it"
            )
        elif lines > recorded:
            problems.append(
                f"{path}: grew to {lines} lines from its baselined {recorded}; "
                "the baseline can only shrink"
            )
    for path in sorted(baseline):
        lines = counts.get(path)
        if lines is None:
            problems.append(f"{path}: baselined but not a measured file; remove the entry")
            continue
        package = package_of(path)
        if package is not None and lines <= THRESHOLDS[package]:
            problems.append(
                f"{path}: {lines} lines is within the {package} threshold of "
                f"{THRESHOLDS[package]}; remove the entry"
            )
    return problems


def shrunk(counts: Mapping[str, int], baseline: Mapping[str, int]) -> list[str]:
    """Baselined paths now shorter than recorded, whose entry can be lowered."""
    return [
        f"{path}: {counts[path]} lines, baselined at {recorded}; lower the entry"
        for path, recorded in sorted(baseline.items())
        if path in counts and counts[path] < recorded
    ]


def repository_files(root: Path) -> Iterable[str]:
    """Tracked and untracked-but-not-ignored files under the package roots."""
    result = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", *THRESHOLDS],
        cwd=root,
        check=True,
        capture_output=True,
    )
    for entry in result.stdout.split(b"\0"):
        if entry:
            yield entry.decode("utf-8", errors="surrogateescape")


def measure(root: Path, paths: Iterable[str]) -> dict[str, int]:
    """Line counts of every measured text file among ``paths``."""
    counts: dict[str, int] = {}
    for path in paths:
        if not is_measured(path):
            continue
        file = root / path
        if not file.is_file():  # deleted from the working tree
            continue
        lines = count_lines(file.read_bytes())
        if lines is not None:
            counts[path] = lines
    return counts


def main(argv: list[str] | None = None) -> int:
    """Run the gate; return the process exit status."""
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument("--root", type=Path, default=REPO_ROOT, help="repository root")
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE, help="baseline file")
    args = parser.parse_args(argv)

    try:
        baseline = parse_baseline(args.baseline.read_text(encoding="utf-8"))
    except (OSError, BaselineError) as error:
        print(f"file-size: cannot read baseline {args.baseline}: {error}", file=sys.stderr)
        return 2
    counts = measure(args.root, repository_files(args.root))

    for notice in shrunk(counts, baseline):
        print(f"file-size: note: {notice}")
    problems = evaluate(counts, baseline)
    for problem in problems:
        print(f"file-size: {problem}", file=sys.stderr)
    if problems:
        print(
            f"file-size: FAILED with {len(problems)} problem(s); baseline: {args.baseline}",
            file=sys.stderr,
        )
        return 1
    print(f"file-size: ok ({len(counts)} files measured, {len(baseline)} baselined)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
