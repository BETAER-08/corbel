import json
import re
import shutil
import subprocess
import time
from pathlib import Path

from enclosing import all_definitions, enclosing_definition

RUST_KEYWORDS = {
    "if", "while", "for", "match", "return", "fn", "let", "async", "await",
    "impl", "struct", "enum", "trait", "pub", "mod", "use", "where", "loop",
    "unsafe", "move", "in", "else",
}
PY_KEYWORDS = {
    "if", "while", "for", "return", "def", "class", "elif", "else", "with",
    "except", "assert", "yield", "lambda", "and", "or", "not", "in", "print",
    "raise", "await", "async",
}

CALL_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")


def _keywords_for(language):
    return RUST_KEYWORDS if language == "rust" else PY_KEYWORDS


def _exclude_dirs(language):
    if language == "rust":
        return [".git", "target"]
    return [".git", "__pycache__", "build", "dist", ".venv", "venv", "*.egg-info", ".mypy_cache", ".pytest_cache"]


def _extension_for(language):
    return "rs" if language == "rust" else "py"


class ToolUnavailable(RuntimeError):
    pass


def _require(binary):
    path = shutil.which(binary)
    if path is None:
        raise ToolUnavailable(f"required binary not found on PATH: {binary}")
    return path


def tool_version(binary, version_flag="--version"):
    path = shutil.which(binary)
    if path is None:
        return None
    try:
        out = subprocess.run(
            [path, version_flag], capture_output=True, text=True, timeout=10
        )
        first_line = (out.stdout or out.stderr).strip().splitlines()
        return first_line[0] if first_line else None
    except Exception:
        return None


def _run(cmd, cwd=None, timeout=120):
    start = time.perf_counter()
    proc = subprocess.run(
        cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout
    )
    elapsed = time.perf_counter() - start
    return proc, elapsed


def _ripgrep_call_sites(repo_root, name, language, search_dir=None):
    rg = _require("rg")
    pattern = rf"\b{re.escape(name)}\s*\("
    cmd = [rg, "-n", "--no-heading", "-t", "rust" if language == "rust" else "py", pattern, str(search_dir or repo_root)]
    proc, elapsed = _run(cmd)
    hits = []
    for line in proc.stdout.splitlines():
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        file_part, line_part, _text = parts
        try:
            line_no = int(line_part)
        except ValueError:
            continue
        hits.append((file_part, line_no))
    return hits, elapsed, " ".join(cmd)


def _system_grep_call_sites(repo_root, name, language, search_dir=None):
    grep = _require("grep")
    pattern = rf"\b{re.escape(name)}\s*\("
    ext = _extension_for(language)
    args = ["-rnE"]
    for d in _exclude_dirs(language):
        args += [f"--exclude-dir={d}"]
    args += [f"--include=*.{ext}"]
    cmd = [grep] + args + [pattern, str(search_dir or repo_root)]
    proc, elapsed = _run(cmd)
    hits = []
    for line in proc.stdout.splitlines():
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        file_part, line_part, _text = parts
        try:
            line_no = int(line_part)
        except ValueError:
            continue
        hits.append((file_part, line_no))
    return hits, elapsed, " ".join(cmd)


def _sites_to_qualified_callers(repo_root, hits, language, def_file, def_line):
    def_file_norm = str(Path(def_file))
    callers = []
    for (file_part, line_no) in hits:
        rel = str(Path(file_part).resolve().relative_to(Path(repo_root).resolve()))
        if rel == def_file_norm and line_no == def_line:
            continue
        abs_path = Path(repo_root) / rel
        entry = enclosing_definition(abs_path, line_no, language)
        qname = entry.qualified_name if entry else f"<module-level:{rel}:{line_no}>"
        callers.append({"file": rel, "line": line_no, "enclosing_symbol": qname})
    return callers


def ripgrep_find_callers(repo_root, name, def_file, def_line, language, search_dir=None):
    hits, elapsed, cmd = _ripgrep_call_sites(repo_root, name, language, search_dir)
    callers = _sites_to_qualified_callers(repo_root, hits, language, def_file, def_line)
    return callers, elapsed, {"command": cmd, "raw_hit_count": len(hits)}


def grep_find_callers(repo_root, name, def_file, def_line, language, search_dir=None):
    hits, elapsed, cmd = _system_grep_call_sites(repo_root, name, language, search_dir)
    callers = _sites_to_qualified_callers(repo_root, hits, language, def_file, def_line)
    return callers, elapsed, {"command": cmd, "raw_hit_count": len(hits)}


def _body_end_line_regex(repo_root, def_file, def_line, language):
    abs_path = Path(repo_root) / def_file
    defs = all_definitions(abs_path, language)
    with open(abs_path, encoding="utf-8", errors="replace") as f:
        total_lines = sum(1 for _ in f)
    next_line = None
    for entry in defs:
        if entry.line > def_line:
            next_line = entry.line
            break
    return (next_line - 1) if next_line else total_lines


def _scan_callees_in_range(repo_root, def_file, start_line, end_line, self_name, language):
    abs_path = Path(repo_root) / def_file
    with open(abs_path, encoding="utf-8", errors="replace") as f:
        lines = f.readlines()
    keywords = _keywords_for(language)
    names = []
    for i in range(start_line, min(end_line, len(lines)) + 1):
        if i - 1 < 0 or i - 1 >= len(lines):
            continue
        text = lines[i - 1]
        for m in CALL_RE.finditer(text):
            candidate = m.group(1)
            if candidate in keywords:
                continue
            if i == start_line and candidate == self_name:
                continue
            names.append(candidate)
    return names


def ripgrep_find_callees(repo_root, name, def_file, def_line, language):
    _require("rg")
    start = time.perf_counter()
    end_line = _body_end_line_regex(repo_root, def_file, def_line, language)
    callees = _scan_callees_in_range(repo_root, def_file, def_line, end_line, name, language)
    elapsed = time.perf_counter() - start
    return callees, elapsed, {"body_range": [def_line, end_line], "method": "regex-indent body scan"}


def grep_find_callees(repo_root, name, def_file, def_line, language):
    _require("grep")
    start = time.perf_counter()
    end_line = _body_end_line_regex(repo_root, def_file, def_line, language)
    callees = _scan_callees_in_range(repo_root, def_file, def_line, end_line, name, language)
    elapsed = time.perf_counter() - start
    return callees, elapsed, {"body_range": [def_line, end_line], "method": "regex-indent body scan"}


def ripgrep_find_definition(repo_root, name, language, search_dir=None):
    def_pattern = (
        rf"^\s*(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+{re.escape(name)}\b"
        if language == "rust"
        else rf"^\s*def\s+{re.escape(name)}\b"
    )
    rg = _require("rg")
    cmd = [rg, "-n", "--no-heading", "-t", "rust" if language == "rust" else "py", def_pattern, str(search_dir or repo_root)]
    proc, elapsed = _run(cmd)
    results = []
    for line in proc.stdout.splitlines():
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        file_part, line_part, _text = parts
        results.append({"file": str(Path(file_part).resolve().relative_to(Path(repo_root).resolve())), "line": int(line_part)})
    return results, elapsed, {"command": " ".join(cmd)}


def grep_find_definition(repo_root, name, language, search_dir=None):
    def_pattern = (
        rf"^\s*(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+{re.escape(name)}\b"
        if language == "rust"
        else rf"^\s*def\s+{re.escape(name)}\b"
    )
    grep = _require("grep")
    ext = _extension_for(language)
    args = ["-rnE"]
    for d in _exclude_dirs(language):
        args += [f"--exclude-dir={d}"]
    args += [f"--include=*.{ext}"]
    cmd = [grep] + args + [def_pattern, str(search_dir or repo_root)]
    proc, elapsed = _run(cmd)
    results = []
    for line in proc.stdout.splitlines():
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        file_part, line_part, _text = parts
        results.append({"file": str(Path(file_part).resolve().relative_to(Path(repo_root).resolve())), "line": int(line_part)})
    return results, elapsed, {"command": " ".join(cmd)}


class CtagsIndex:
    def __init__(self, repo_root, language, search_dir=None):
        self.repo_root = Path(repo_root)
        self.search_dir = Path(search_dir) if search_dir else self.repo_root
        self.language = language
        self.by_name = {}
        self.by_file = {}
        self.build_time_s = 0.0
        self._build()

    def _build(self):
        ctags = _require("ctags")
        cmd = [ctags, "-R", "--fields=+znKe", "-f", "-", str(self.search_dir)]
        proc, elapsed = _run(cmd, timeout=180)
        self.build_time_s = elapsed
        self.command = " ".join(cmd)
        for line in proc.stdout.splitlines():
            fields = line.split("\t")
            if len(fields) < 4:
                continue
            name = fields[0]
            file_part = fields[1]
            rest = fields[3:]
            kind = None
            line_no = None
            scope = None
            end_line = None
            for f in rest:
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
                elif ":" in f:
                    key, _, value = f.partition(":")
                    if key in ("class", "implementation", "struct", "interface", "trait"):
                        scope = value
            if line_no is None:
                continue
            try:
                rel = str(Path(file_part).resolve().relative_to(self.repo_root.resolve()))
            except ValueError:
                continue
            qname = f"{scope}::{name}" if (scope and self.language == "rust") else (
                f"{scope}.{name}" if scope else name
            )
            record = {
                "name": name,
                "qualified_name": qname,
                "file": rel,
                "line": line_no,
                "kind": kind,
                "end_line": end_line,
            }
            self.by_name.setdefault(name, []).append(record)
            self.by_file.setdefault(rel, []).append(record)
        for rel in self.by_file:
            self.by_file[rel].sort(key=lambda r: r["line"])

    def enclosing(self, rel_file, target_line):
        candidates = [
            r for r in self.by_file.get(rel_file, [])
            if r["kind"] in ("function", "method", "member") and r["line"] <= target_line
        ]
        if not candidates:
            return None
        return max(candidates, key=lambda r: r["line"])

    def body_end(self, rel_file, def_line):
        records = self.by_file.get(rel_file, [])
        own = next((r for r in records if r["line"] == def_line), None)
        if own and own.get("end_line"):
            return own["end_line"]
        following = [r["line"] for r in records if r["line"] > def_line]
        if following:
            return min(following) - 1
        abs_path = self.repo_root / rel_file
        with open(abs_path, encoding="utf-8", errors="replace") as f:
            return sum(1 for _ in f)

    def definitions_named(self, name):
        return self.by_name.get(name, [])


def ctags_find_callers(repo_root, name, def_file, def_line, language, index, search_dir=None):
    hits, elapsed_rg, _cmd = (
        _ripgrep_call_sites(repo_root, name, language, search_dir)
        if shutil.which("rg")
        else _system_grep_call_sites(repo_root, name, language, search_dir)
    )
    def_file_norm = str(Path(def_file))
    callers = []
    for (file_part, line_no) in hits:
        rel = str(Path(file_part).resolve().relative_to(Path(repo_root).resolve()))
        if rel == def_file_norm and line_no == def_line:
            continue
        entry = index.enclosing(rel, line_no)
        if entry is None:
            continue
        callers.append({"file": rel, "line": line_no, "enclosing_symbol": entry["qualified_name"]})
    return callers, elapsed_rg, {"raw_hit_count": len(hits), "ctags_filtered_count": len(callers)}


def ctags_find_callees(repo_root, name, def_file, def_line, language, index):
    start = time.perf_counter()
    end_line = index.body_end(def_file, def_line)
    callees = _scan_callees_in_range(repo_root, def_file, def_line, end_line, name, language)
    elapsed = time.perf_counter() - start
    return callees, elapsed, {"body_range": [def_line, end_line], "method": "ctags end-field or next-tag boundary"}


def ctags_find_definition(name, index):
    start = time.perf_counter()
    results = [{"file": r["file"], "line": r["line"]} for r in index.definitions_named(name) if r["kind"] in ("function", "method", "member")]
    elapsed = time.perf_counter() - start
    return results, elapsed, {"method": "ctags tag lookup by name"}


class CorbelClient:
    def __init__(self, binary_path, repo_root):
        self.binary_path = str(binary_path)
        self.repo_root = str(repo_root)
        self._id = 0
        self.proc = None

    def start(self):
        self.proc = subprocess.Popen(
            [self.binary_path, "serve", self.repo_root],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            bufsize=1,
        )
        self._request("initialize", {"protocolVersion": "2025-06-18"})
        self._notify("notifications/initialized")

    def close(self):
        if self.proc is None:
            return
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()

    def _notify(self, method):
        payload = json.dumps({"jsonrpc": "2.0", "method": method})
        self.proc.stdin.write(payload + "\n")
        self.proc.stdin.flush()

    def _request(self, method, params=None):
        self._id += 1
        req = {"jsonrpc": "2.0", "id": self._id, "method": method}
        if params is not None:
            req["params"] = params
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("corbel serve produced no response (process may have exited)")
        return json.loads(line)

    def call_tool(self, name, arguments):
        resp = self._request("tools/call", {"name": name, "arguments": arguments})
        if "error" in resp:
            raise RuntimeError(f"corbel tool {name} returned error: {resp['error']}")
        text = resp["result"]["content"][0]["text"]
        return json.loads(text)

    def get_symbol(self, name, file=None, line=None):
        args = {"name": name}
        if file is not None:
            args["file"] = file
        if line is not None:
            args["line"] = line
        return self.call_tool("get_symbol", args)


def corbel_index(binary_path, repo_root):
    cmd = [str(binary_path), "index", str(repo_root)]
    proc, elapsed = _run(cmd, timeout=300)
    if proc.returncode != 0:
        raise RuntimeError(f"corbel index failed: {proc.stderr}")
    return elapsed, proc.stdout


def corbel_find_callers(client, name, def_file, def_line):
    start = time.perf_counter()
    payload = client.get_symbol(name, file=def_file, line=def_line)
    elapsed = time.perf_counter() - start
    if not payload.get("found") or not payload.get("results"):
        return [], elapsed, {"payload": payload}
    result = payload["results"][0]
    callers = [
        {"file": c["file"], "line": c["line"], "enclosing_symbol": c["name"], "resolution": c["resolution"]}
        for c in result.get("callers", [])
    ]
    return callers, elapsed, {"raw_caller_count": len(result.get("callers", []))}


def corbel_find_callees(client, name, def_file, def_line):
    start = time.perf_counter()
    payload = client.get_symbol(name, file=def_file, line=def_line)
    elapsed = time.perf_counter() - start
    if not payload.get("found") or not payload.get("results"):
        return [], elapsed, {"payload": payload}
    result = payload["results"][0]
    callees = [c["name"] for c in result.get("callees", [])]
    return callees, elapsed, {"raw": result.get("callees", [])}


def corbel_find_definition(client, name):
    start = time.perf_counter()
    payload = client.get_symbol(name)
    elapsed = time.perf_counter() - start
    if not payload.get("found"):
        return [], elapsed, {"payload": payload}
    results = [{"file": r["file"], "line": r["line"]} for r in payload["results"]]
    return results, elapsed, {"raw_count": len(results)}
