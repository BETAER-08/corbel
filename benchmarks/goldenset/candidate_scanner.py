"""Enumerate golden-set candidate symbols and tag them with a difficulty
guess, using only ctags and ripgrep.

This module must never import corbel, corbel's MCP client, or
`benchmarks/harness/tool_adapters.py` (which does both). That is not a
style preference: the golden set exists to check corbel against an
independently-derived answer, so nothing that decides which symbols look
"interesting" or how hard they look may consult corbel's own output. The
enforcement is structural — grep this file's imports, not a policy
document, to confirm the isolation holds.

`easy`/`medium`/`hard` are assigned automatically from mechanical signals
(name collision count, raw call-site count). `adversarial` is never
auto-assigned: this script only raises flags (dyn/trait keywords nearby,
decorators, dunder names, etc.) for a human to curate from, per the
golden-set design doc's rule that adversarial cases are hand-picked, not
sampled.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

FUNCTION_KINDS = {"function", "method", "member"}

EXT_FOR_LANGUAGE = {
    "rust": "rs",
    "python": "py",
    "typescript": "ts",
    "tsx": "tsx",
    "javascript": "js",
}

EXCLUDE_DIRS_FOR_LANGUAGE = {
    "rust": [".git", "target"],
    "python": [".git", "__pycache__", "build", "dist", ".venv", "venv", "*.egg-info"],
    "typescript": [".git", "node_modules", "dist", "build"],
    "tsx": [".git", "node_modules", "dist", "build"],
    "javascript": [".git", "node_modules", "dist", "build"],
}

# Heuristic, human-curation-only signals. None of these decide difficulty
# by themselves; they only get surfaced in `adversarial_flags` for a
# person to look at before hand-selecting adversarial entries.
ADVERSARIAL_PATTERNS = {
    "rust": [
        ("dyn_trait_nearby", re.compile(r"\bdyn\s+\w")),
        ("macro_context", re.compile(r"\b\w+!\s*[\[({]")),
        ("generic_bound", re.compile(r"<[^>]*:\s*\w+[^>]*>")),
    ],
    "python": [
        ("dunder_name", re.compile(r"__\w+__")),
        ("getattr_nearby", re.compile(r"\bgetattr\s*\(")),
        ("decorator_present", re.compile(r"^\s*@\w")),
    ],
    "typescript": [
        ("interface_or_implements", re.compile(r"\b(interface|implements)\b")),
        ("higher_order_callback", re.compile(r"=>\s*[\w.]+\(")),
    ],
    "tsx": [
        ("interface_or_implements", re.compile(r"\b(interface|implements)\b")),
        ("higher_order_callback", re.compile(r"=>\s*[\w.]+\(")),
    ],
    "javascript": [
        ("prototype_chain", re.compile(r"\.prototype\.")),
        ("higher_order_callback", re.compile(r"=>\s*[\w.]+\(")),
    ],
}


def _require(binary: str) -> str:
    path = shutil.which(binary)
    if path is None:
        raise RuntimeError(f"required binary not found on PATH: {binary}")
    return path


@dataclass
class Definition:
    name: str
    file: str
    line: int
    kind: str
    end_line: int | None = None


@dataclass
class Candidate:
    name: str
    file: str
    line: int
    kind: str
    language: str
    name_count: int
    raw_caller_count: int
    difficulty_guess: str
    adversarial_flags: list[str] = field(default_factory=list)


def _run_ctags(repo_path: Path, language: str, scope_subdir: str | None) -> list[Definition]:
    # universal-ctags auto-detects language per file and will happily index
    # e.g. Python helper scripts sitting inside a Rust repo (observed in
    # hyperfine/scripts/*.py) alongside the .rs sources. Restrict --languages
    # so candidates are only ever drawn from the target language's own files.
    ctags_language = {
        "rust": "Rust",
        "python": "Python",
        "typescript": "TypeScript",
        "tsx": "TSX",
        "javascript": "JavaScript",
    }[language]
    ctags = _require("ctags")
    # scope_subdir narrows *what ctags walks* (e.g. a single package inside a
    # monorepo) while `file` values in the output stay relative to repo_path,
    # not to the subdir - so downstream paths match what lsp_to_draft.py and
    # the golden-file "search_root" convention expect (repo_path stays the
    # LSP/tsconfig root; scope_subdir is a candidate-selection filter only).
    scan_target = (repo_path / scope_subdir) if scope_subdir else repo_path
    cmd = [ctags, "-R", f"--languages={ctags_language}", "--fields=+znKe", "-f", "-", str(scan_target)]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=180, check=True)
    defs: list[Definition] = []
    for line in proc.stdout.splitlines():
        fields_ = line.split("\t")
        if len(fields_) < 4:
            continue
        name = fields_[0]
        file_part = fields_[1]
        kind = None
        line_no = None
        end_line = None
        for f in fields_[3:]:
            if f.startswith("kind:"):
                kind = f[len("kind:"):]
            elif f.startswith("line:"):
                try:
                    line_no = int(f[len("line:"):])
                except ValueError:
                    pass
            elif f.startswith("end:"):
                try:
                    end_line = int(f[len("end:"):])
                except ValueError:
                    pass
        if kind not in FUNCTION_KINDS or line_no is None:
            continue
        try:
            rel = str(Path(file_part).resolve().relative_to(repo_path.resolve()))
        except ValueError:
            continue
        defs.append(Definition(name=name, file=rel, line=line_no, kind=kind, end_line=end_line))
    return defs


def _raw_caller_count(repo_path: Path, name: str, language: str, def_sites: set[tuple[str, int]]) -> int:
    rg = _require("rg")
    ext = EXT_FOR_LANGUAGE[language]
    pattern = rf"\b{re.escape(name)}\s*\("
    cmd = [rg, "-n", "--no-heading", "-g", f"*.{ext}"]
    for d in EXCLUDE_DIRS_FOR_LANGUAGE[language]:
        cmd += ["-g", f"!{d}"]
    cmd += [pattern, str(repo_path)]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    count = 0
    for line in proc.stdout.splitlines():
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        file_part, line_part = parts[0], parts[1]
        try:
            line_no = int(line_part)
        except ValueError:
            continue
        try:
            rel = str(Path(file_part).resolve().relative_to(repo_path.resolve()))
        except ValueError:
            continue
        if (rel, line_no) in def_sites:
            continue
        count += 1
    return count


def _adversarial_flags(repo_path: Path, defn: Definition, language: str) -> list[str]:
    patterns = ADVERSARIAL_PATTERNS.get(language, [])
    if not patterns:
        return []
    abs_path = repo_path / defn.file
    try:
        with open(abs_path, encoding="utf-8", errors="replace") as f:
            lines = f.readlines()
    except OSError:
        return []
    start = max(0, defn.line - 6)
    end = min(len(lines), (defn.end_line or defn.line + 30))
    window = "".join(lines[start:end])
    flags = []
    for flag_name, regex in patterns:
        if regex.search(window):
            flags.append(flag_name)
    return flags


def _difficulty(name_count: int, raw_caller_count: int) -> str:
    if name_count == 1 and raw_caller_count <= 5:
        return "easy"
    if name_count >= 3 or raw_caller_count > 20:
        return "hard"
    return "medium"


def scan(repo_path: Path, language: str, scope_subdir: str | None = None) -> list[Candidate]:
    defs = _run_ctags(repo_path, language, scope_subdir)
    by_name: dict[str, list[Definition]] = defaultdict(list)
    for d in defs:
        by_name[d.name].append(d)

    def_sites = {(d.file, d.line) for d in defs}

    candidates: list[Candidate] = []
    for d in defs:
        name_count = len(by_name[d.name])
        raw_caller_count = _raw_caller_count(repo_path, d.name, language, def_sites)
        difficulty = _difficulty(name_count, raw_caller_count)
        flags = _adversarial_flags(repo_path, d, language)
        candidates.append(
            Candidate(
                name=d.name,
                file=d.file,
                line=d.line,
                kind=d.kind,
                language=language,
                name_count=name_count,
                raw_caller_count=raw_caller_count,
                difficulty_guess=difficulty,
                adversarial_flags=flags,
            )
        )
    candidates.sort(key=lambda c: (c.file, c.line))
    return candidates


def main() -> None:
    parser = argparse.ArgumentParser(
        description=(
            "List golden-set candidates with a mechanical difficulty guess. "
            "Never imports or invokes corbel."
        )
    )
    parser.add_argument("repo_path", type=Path)
    parser.add_argument("--language", required=True, choices=sorted(EXT_FOR_LANGUAGE))
    parser.add_argument(
        "--scope-subdir",
        default=None,
        help="restrict candidate definitions to this subdir of repo_path (e.g. a single "
        "package inside a monorepo); repo_path itself stays the path base for the "
        "'file' field and for raw_caller_count's search, so it can stay the LSP/tsconfig root",
    )
    parser.add_argument("--difficulty", choices=["easy", "medium", "hard"], default=None)
    parser.add_argument("--only-flagged", action="store_true", help="only print candidates with adversarial_flags")
    parser.add_argument("-o", "--output", type=Path, default=None)
    args = parser.parse_args()

    candidates = scan(args.repo_path.resolve(), args.language, args.scope_subdir)

    if args.difficulty:
        candidates = [c for c in candidates if c.difficulty_guess == args.difficulty]
    if args.only_flagged:
        candidates = [c for c in candidates if c.adversarial_flags]

    payload = [
        {
            "name": c.name,
            "file": c.file,
            "line": c.line,
            "kind": c.kind,
            "language": c.language,
            "name_count": c.name_count,
            "raw_caller_count": c.raw_caller_count,
            "difficulty_guess": c.difficulty_guess,
            "adversarial_flags": c.adversarial_flags,
        }
        for c in candidates
    ]

    out = json.dumps(payload, indent=2)
    if args.output:
        args.output.write_text(out + "\n", encoding="utf-8")
        print(f"wrote {len(payload)} candidates to {args.output}", file=sys.stderr)
    else:
        print(out)


if __name__ == "__main__":
    main()
