import re
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

RUST_FN_RE = re.compile(
    r"^(?P<indent>\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
RUST_IMPL_RE = re.compile(
    r"^(?P<indent>\s*)impl(?:<[^>]*>)?\s+(?:[\w:<>,'\s]+\s+for\s+)?(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
PY_DEF_RE = re.compile(r"^(?P<indent>\s*)def\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)")
PY_CLASS_RE = re.compile(r"^(?P<indent>\s*)class\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)")


@dataclass(frozen=True)
class DefEntry:
    line: int
    qualified_name: str
    bare_name: str


def _indent_len(text):
    return len(text) - len(text.lstrip(" \t"))


def _build_rust_index(lines):
    owners = []
    for i, line in enumerate(lines, start=1):
        m = RUST_IMPL_RE.match(line)
        if m:
            owners.append((i, _indent_len(m.group("indent")), m.group("name")))

    defs = []
    for i, line in enumerate(lines, start=1):
        m = RUST_FN_RE.match(line)
        if not m:
            continue
        indent = _indent_len(m.group("indent"))
        name = m.group("name")
        owner = None
        for (oline, oindent, oname) in reversed(owners):
            if oline < i and oindent < indent:
                owner = oname
                break
        qname = f"{owner}::{name}" if owner else name
        defs.append(DefEntry(line=i, qualified_name=qname, bare_name=name))
    return defs


def _build_python_index(lines):
    classes = []
    for i, line in enumerate(lines, start=1):
        m = PY_CLASS_RE.match(line)
        if m:
            classes.append((i, _indent_len(m.group("indent")), m.group("name")))

    defs = []
    for i, line in enumerate(lines, start=1):
        m = PY_DEF_RE.match(line)
        if not m:
            continue
        indent = _indent_len(m.group("indent"))
        name = m.group("name")
        owner = None
        for (cline, cindent, cname) in reversed(classes):
            if cline < i and cindent < indent:
                owner = cname
                break
        qname = f"{owner}.{name}" if owner else name
        defs.append(DefEntry(line=i, qualified_name=qname, bare_name=name))
    return defs


def language_for_path(path):
    suffix = Path(path).suffix
    if suffix == ".py":
        return "python"
    if suffix in (".rs",):
        return "rust"
    return None


@lru_cache(maxsize=None)
def _index_for_file(abs_path, language):
    with open(abs_path, encoding="utf-8", errors="replace") as f:
        lines = f.readlines()
    if language == "rust":
        return tuple(_build_rust_index(lines))
    if language == "python":
        return tuple(_build_python_index(lines))
    return ()


def enclosing_definition(abs_path, target_line, language, exclude_line=None):
    entries = _index_for_file(str(abs_path), language)
    best = None
    for entry in entries:
        if exclude_line is not None and entry.line == exclude_line:
            continue
        if entry.line <= target_line:
            best = entry
        else:
            break
    return best


def all_definitions(abs_path, language):
    return _index_for_file(str(abs_path), language)
