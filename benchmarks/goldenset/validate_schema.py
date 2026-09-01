"""Validate benchmarks/golden/*.json against SCHEMA.md's rules.

Checks performed (see SCHEMA.md "Validation" section):
  1. Every entry has the required fields (id, symbol, category, tasks,
     verification), and tasks/verification have their required sub-fields.
  2. Every easy/medium entry has tasks.impact == null; every hard/adversarial
     entry has tasks.impact != null.
  3. Every adversarial entry has a non-null verification.reverification with
     agrees_with_first_pass set (and a note when it is false).
  4. Every entry's verification.verification_method (or the file's top-level
     verification_method) contains an explicit "corbel was never executed"
     attestation.
  5. Every file's top-level "commit" matches the actual HEAD of the cloned
     repo at local_path, if that repo is present on disk (skipped with a
     warning otherwise - this script never clones or fetches anything).
  6. No duplicate "id" across all files combined.
  7. Reports the actual difficulty distribution vs. the golden-set's overall
     120-entry target (36/48/24/12) - informational, not a hard failure.

This script never imports or invokes corbel, and never runs any git command
that could mutate a repo (read-only `git rev-parse HEAD` only).
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

TARGET_DISTRIBUTION = {"easy": 36, "medium": 48, "hard": 24, "adversarial": 12}

REQUIRED_ENTRY_FIELDS = ["id", "symbol", "category", "tasks", "verification"]
REQUIRED_SYMBOL_FIELDS = ["name", "file", "line", "kind"]
REQUIRED_VERIFICATION_FIELDS = [
    "verified_by",
    "verification_date",
    "verification_method",
    "verification_note",
    "reverification",
]
ATTESTATION_SUBSTRING = "corbel was never executed"


def _repo_head(local_path: Path) -> str | None:
    if not (local_path / ".git").exists():
        return None
    try:
        out = subprocess.run(
            ["git", "-C", str(local_path), "rev-parse", "HEAD"],
            capture_output=True, text=True, timeout=10, check=True,
        )
        return out.stdout.strip()
    except (subprocess.CalledProcessError, OSError):
        return None


def validate_file(path: Path, repos_root: Path) -> tuple[list[str], Counter]:
    errors: list[str] = []
    data = json.loads(path.read_text())

    if "corbel was never executed" not in data.get("verification_method", "") + "".join(
        e.get("verification", {}).get("verification_note", "") for e in data.get("entries", [])
    ):
        # top-level attestation is the authoritative one; per-entry text is a bonus, not required
        if ATTESTATION_SUBSTRING not in data.get("verification_method", ""):
            errors.append(f"{path.name}: top-level verification_method missing '{ATTESTATION_SUBSTRING}' attestation")

    local_path = data.get("local_path")
    commit = data.get("commit")
    if local_path and commit:
        repo_dir = repos_root / Path(local_path).name
        head = _repo_head(repo_dir)
        if head is None:
            print(f"  (warning) {path.name}: repo at {repo_dir} not present/not a git repo - skipping commit-match check", file=sys.stderr)
        elif head != commit:
            errors.append(f"{path.name}: top-level commit {commit} != repo HEAD {head} at {repo_dir}")

    diff_counts: Counter = Counter()
    entries = data.get("entries", [])
    for e in entries:
        eid = e.get("id", "<missing id>")
        for field in REQUIRED_ENTRY_FIELDS:
            if field not in e:
                errors.append(f"{path.name}:{eid}: missing required field '{field}'")
        symbol = e.get("symbol", {})
        for field in REQUIRED_SYMBOL_FIELDS:
            if field not in symbol:
                errors.append(f"{path.name}:{eid}: symbol missing field '{field}'")

        tasks = e.get("tasks", {})
        if "impact" not in tasks:
            errors.append(f"{path.name}:{eid}: tasks missing 'impact' key")
        if "callers" not in tasks and "callees" not in tasks:
            errors.append(f"{path.name}:{eid}: tasks has neither 'callers' nor 'callees'")

        verification = e.get("verification", {})
        for field in REQUIRED_VERIFICATION_FIELDS:
            if field not in verification:
                errors.append(f"{path.name}:{eid}: verification missing field '{field}'")

        difficulty = e.get("difficulty")
        if difficulty is None:
            errors.append(f"{path.name}:{eid}: difficulty is null/missing (legacy entry not yet classified)")
        else:
            diff_counts[difficulty] += 1
            impact = tasks.get("impact")
            if difficulty in ("easy", "medium"):
                if impact is not None:
                    errors.append(f"{path.name}:{eid}: difficulty={difficulty} but tasks.impact is non-null")
            elif difficulty in ("hard", "adversarial"):
                if impact is None:
                    errors.append(f"{path.name}:{eid}: difficulty={difficulty} but tasks.impact is null")
            else:
                errors.append(f"{path.name}:{eid}: unknown difficulty value '{difficulty}'")

            if difficulty == "adversarial":
                reverif = verification.get("reverification")
                if reverif is None:
                    errors.append(f"{path.name}:{eid}: adversarial entry missing verification.reverification")
                else:
                    reverif_note = reverif.get("note", "")
                    if not any(kw in reverif_note for kw in ("subagent", "Agent", "isolated", "independent")):
                        errors.append(
                            f"{path.name}:{eid}: reverification.note doesn't state its independence "
                            f"methodology (expected a mention of an isolated/independent subagent pass, "
                            f"per SCHEMA.md's 'Verifier independence' section) - passing agrees_with_first_pass "
                            f"alone doesn't establish HOW the second pass was kept independent of the first"
                        )
                    if "agrees_with_first_pass" not in reverif:
                        errors.append(f"{path.name}:{eid}: reverification missing 'agrees_with_first_pass'")
                    elif reverif["agrees_with_first_pass"] is False and not reverif.get("note"):
                        errors.append(f"{path.name}:{eid}: reverification disagrees but has no explanatory note")
            else:
                if verification.get("reverification") is not None:
                    errors.append(f"{path.name}:{eid}: non-adversarial entry has non-null verification.reverification")

        # per-entry verification_method attestation (soft check, informational only,
        # since the file-level attestation above is the one SCHEMA.md actually requires)

    return errors, diff_counts


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("golden_dir", type=Path, nargs="?", default=Path("benchmarks/golden"))
    parser.add_argument("--repos-root", type=Path, default=Path("benchmarks/repos"),
                         help="where cloned repos live, for the commit-match check (skipped if a repo isn't present)")
    args = parser.parse_args()

    files = sorted(args.golden_dir.glob("*.json"))
    if not files:
        print(f"no *.json files found under {args.golden_dir}", file=sys.stderr)
        sys.exit(2)

    all_errors: list[str] = []
    all_ids: dict[str, str] = {}
    total_counts: Counter = Counter()

    for path in files:
        errors, counts = validate_file(path, args.repos_root)
        all_errors.extend(errors)
        total_counts.update(counts)

        data = json.loads(path.read_text())
        for e in data.get("entries", []):
            eid = e.get("id")
            if eid is None:
                continue
            if eid in all_ids:
                all_errors.append(f"duplicate id '{eid}' in {path.name} (first seen in {all_ids[eid]})")
            else:
                all_ids[eid] = path.name

    print(f"Checked {len(files)} file(s), {len(all_ids)} entries with an id.\n")

    print("Difficulty distribution (tagged entries only):")
    total_tagged = sum(total_counts.values())
    for k in ("easy", "medium", "hard", "adversarial"):
        got = total_counts.get(k, 0)
        target = TARGET_DISTRIBUTION[k]
        marker = "OK" if got == target else f"DIFF {got - target:+d}"
        print(f"  {k:12s} {got:3d} / {target:3d}  [{marker}]")
    print(f"  {'total':12s} {total_tagged:3d} / {sum(TARGET_DISTRIBUTION.values()):3d}")
    print()

    if all_errors:
        print(f"FAILED: {len(all_errors)} issue(s) found:\n")
        for err in all_errors:
            print(f"  - {err}")
        sys.exit(1)
    else:
        print("PASSED: no schema issues found.")


if __name__ == "__main__":
    main()
