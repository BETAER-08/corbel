"""Turn `candidate_scanner.py` output into unverified golden-set drafts
by asking a real LSP server (rust-analyzer / pyright-langserver) for
references and enclosing symbols.

This is a draft generator, not a source of truth. Every entry it writes
carries `"verified_by": "lsp-draft (unverified)"` so it cannot be mistaken
for a finished golden-set entry — a human must run the C procedure
(ripgrep cross-check + manual read) and overwrite that field before an
entry is committed to `benchmarks/golden/*.json`.

Like `candidate_scanner.py`, this module never imports corbel or
`benchmarks/harness/tool_adapters.py`.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import time
from pathlib import Path

from lsp_client import LspClient, LspTimeout

LANGUAGE_ID = {
    "rust": "rust",
    "python": "python",
    "typescript": "typescript",
    "tsx": "typescriptreact",
    "javascript": "javascript",
}

QUALIFIER_SEP = {
    "rust": "::",
    "python": ".",
    "typescript": ".",
    "tsx": ".",
    "javascript": ".",
}

DEFAULT_LSP_CMD = {
    "rust": ["rust-analyzer"],
    "python": ["pyright-langserver", "--stdio"],
}

FILE_EXT = {
    "rust": "rs",
    "python": "py",
    "typescript": "ts",
    "tsx": "tsx",
    "javascript": "js",
}


def _open_whole_workspace(client: LspClient, repo_path: Path, language: str) -> int:
    """Some servers (observed with pyright) only include a file in
    cross-file reference search once that file has been opened at least
    once — being under rootUri is not sufficient by itself. Open every
    source file up front rather than lazily, or references silently
    under-collect with no error, which is worse than a slow warmup."""
    ext = FILE_EXT[language]
    lang_id = LANGUAGE_ID[language]
    count = 0
    for path in sorted(repo_path.rglob(f"*.{ext}")):
        if any(part in {".git", "node_modules", "__pycache__", "target", ".venv", "venv"} for part in path.parts):
            continue
        client.did_open(path, lang_id)
        count += 1
    return count


def _line_col_of_name(abs_path: Path, line_1indexed: int, name: str) -> int:
    lines = abs_path.read_text(encoding="utf-8", errors="replace").splitlines()
    if line_1indexed - 1 >= len(lines):
        raise ValueError(f"line {line_1indexed} out of range for {abs_path}")
    text = lines[line_1indexed - 1]
    m = re.search(rf"\b{re.escape(name)}\b", text)
    if not m:
        raise ValueError(f"name {name!r} not found on {abs_path}:{line_1indexed}")
    return m.start()


def _uri_to_relpath(uri: str, repo_path: Path) -> str:
    p = Path(uri.replace("file://", ""))
    return str(p.resolve().relative_to(repo_path.resolve()))


def _position_in_range(pos: dict, rng: dict) -> bool:
    start, end = rng["start"], rng["end"]

    def le(a, b):
        return (a["line"], a["character"]) <= (b["line"], b["character"])

    return le(start, pos) and le(pos, end)


def _find_innermost(symbols: list[dict], pos: dict, parent: dict | None = None):
    best = None
    for sym in symbols:
        rng = sym.get("range") or sym.get("location", {}).get("range")
        if rng and _position_in_range(pos, rng):
            children = sym.get("children") or []
            deeper = _find_innermost(children, pos, sym)
            best = deeper if deeper is not None else (sym, parent)
    return best


_RUST_IMPL_FOR = re.compile(r"^impl(?:<[^>]*>)?\s+[\w:]+(?:<[^>]*>)?\s+for\s+([\w:]+)")
_RUST_IMPL_PLAIN = re.compile(r"^impl(?:<[^>]*>)?\s+([\w:]+)")

# LSP SymbolKind values (see the LSP spec) for constructs that can actually
# contain a call expression in their body. A reference whose innermost
# containing documentSymbol is anything else (Module=2, Namespace=3,
# Class=5, Interface=11, ...) is not inside a function body at all - most
# commonly a `use`/`import` statement that happens to sit inside a module
# block (e.g. `mod tests { use foo::bar; ... }` in Rust), which still gets
# a non-None enclosing symbol from a naive containment walk even though it
# is not a call site. Top-level imports outside any module already produce
# None correctly; this closes the same gap for imports nested one level in.
_CALLABLE_SYMBOL_KINDS = {6, 9, 12}  # Method, Constructor, Function


def _is_callable_kind(sym: dict) -> bool:
    return sym.get("kind") in _CALLABLE_SYMBOL_KINDS


def _owner_name(parent_name: str, language: str) -> str:
    """rust-analyzer's documentSymbol names an impl block's container
    symbol as the full `impl Trait for Type<'a>` (or `impl Type<'a>`)
    header text. The golden-set convention is the bare Self type
    (`Owner::method`, matching corbel's own `impl_name::method` style and
    existing golden entries like `ShellExecutor::run_command_and_measure`),
    so strip the `impl`/`for`/generics noise for Rust specifically."""
    if language != "rust":
        return parent_name
    m = _RUST_IMPL_FOR.match(parent_name) or _RUST_IMPL_PLAIN.match(parent_name)
    return m.group(1) if m else parent_name


def _qualified_name(sym: dict, parent: dict | None, language: str) -> str:
    name = sym["name"]
    if parent is None:
        return name
    sep = QUALIFIER_SEP.get(language, ".")
    return f"{_owner_name(parent['name'], language)}{sep}{name}"


def _def_line_1indexed(sym: dict) -> int:
    sel = sym.get("selectionRange") or sym["range"]
    return sel["start"]["line"] + 1


def draft_entry(
    client: LspClient,
    repo_path: Path,
    language: str,
    candidate: dict,
) -> dict:
    name = candidate["name"]
    file_rel = candidate["file"]
    def_line = candidate["line"]
    abs_path = repo_path / file_rel
    lang_id = LANGUAGE_ID[language]

    client.did_open(abs_path, lang_id)
    col = _line_col_of_name(abs_path, def_line, name)
    uri = abs_path.resolve().as_uri()

    refs = client.references(uri, def_line - 1, col)

    callers = []
    doc_symbol_cache: dict[str, list[dict]] = {}
    for ref in refs:
        ref_uri = ref["uri"]
        ref_pos = ref["range"]["start"]
        try:
            ref_rel = _uri_to_relpath(ref_uri, repo_path)
        except ValueError:
            continue  # reference outside the repo (stdlib/vendored) — not a caller entry

        if ref_uri not in doc_symbol_cache:
            ref_abs = Path(ref_uri.replace("file://", ""))
            client.did_open(ref_abs, lang_id)
            doc_symbol_cache[ref_uri] = client.document_symbol(ref_uri)
        symbols = doc_symbol_cache[ref_uri]

        found = _find_innermost(symbols, ref_pos)
        if found is None or not _is_callable_kind(found[0]):
            callers.append(
                {
                    "file": ref_rel,
                    "line": None,
                    "enclosing_symbol": None,
                    "call_line": ref_pos["line"] + 1,
                    "note": "LSP draft: no enclosing symbol found by documentSymbol; needs manual read",
                }
            )
            continue

        sym, parent = found
        if sym["name"] == name and _def_line_1indexed(sym) == def_line and ref_rel == file_rel:
            continue  # the declaration itself, not a call site

        callers.append(
            {
                "file": ref_rel,
                "line": _def_line_1indexed(sym),
                "enclosing_symbol": _qualified_name(sym, parent, language),
                "call_line": ref_pos["line"] + 1,
            }
        )

    return {
        "id": None,
        "repo": None,
        "commit": None,
        "language": language,
        "difficulty": candidate.get("difficulty_guess"),
        "category": None,
        "symbol": {"name": name, "file": file_rel, "line": def_line, "kind": candidate.get("kind")},
        "tasks": {"callers": callers, "callees": [], "impact": None},
        "verification": {
            "verified_by": "lsp-draft (unverified)",
            "verification_date": None,
            "verification_method": None,
            "verification_note": (
                f"DRAFT ONLY. Generated by lsp_to_draft.py from "
                f"{'rust-analyzer' if language == 'rust' else 'pyright'} references + "
                f"documentSymbol. Not cross-checked with ripgrep, not manually read. "
                f"Do not commit without running the C procedure."
            ),
            "reverification": None,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Draft golden-set entries from LSP references. Output is UNVERIFIED.")
    parser.add_argument("repo_path", type=Path)
    parser.add_argument("--language", required=True, choices=sorted(LANGUAGE_ID))
    parser.add_argument("--candidates", type=Path, required=True, help="candidate_scanner.py JSON output")
    parser.add_argument("--lsp-cmd", nargs="+", default=None, help="override LSP server command")
    parser.add_argument("-o", "--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=None)
    parser.add_argument(
        "--warmup-seconds",
        type=float,
        default=10.0,
        help="sleep after initialize to let the server finish its background workspace scan "
        "before the first references/documentSymbol request; too short under-collects "
        "cross-file references silently",
    )
    args = parser.parse_args()

    repo_path = args.repo_path.resolve()
    candidates = json.loads(args.candidates.read_text(encoding="utf-8"))
    if args.limit:
        candidates = candidates[: args.limit]

    lsp_cmd = args.lsp_cmd or DEFAULT_LSP_CMD.get(args.language)
    if not lsp_cmd:
        print(f"error: no default LSP command for {args.language}; pass --lsp-cmd", file=sys.stderr)
        sys.exit(1)

    client = LspClient(lsp_cmd, cwd=repo_path)
    client.initialize(repo_path)
    opened = _open_whole_workspace(client, repo_path, args.language)
    print(f"opened {opened} files; waiting {args.warmup_seconds}s for workspace indexing...", file=sys.stderr)
    time.sleep(args.warmup_seconds)

    drafts = []
    for i, cand in enumerate(candidates):
        try:
            drafts.append(draft_entry(client, repo_path, args.language, cand))
        except (LspTimeout, ValueError, RuntimeError) as exc:
            print(f"skip {cand.get('name')} ({cand.get('file')}:{cand.get('line')}): {exc}", file=sys.stderr)
        if (i + 1) % 10 == 0:
            print(f"...{i + 1}/{len(candidates)}", file=sys.stderr)

    client.shutdown()

    args.output.write_text(json.dumps(drafts, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {len(drafts)} draft entries to {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
