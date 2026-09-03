import json
import os
import random
import shutil
import sqlite3
import statistics
import subprocess
import sys
import time
from pathlib import Path

CORBEL = None
LANG_EXTS = {"rs", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs"}


def source_size_bytes(repo_path):
    total = 0
    for root, dirs, files in os.walk(repo_path):
        dirs[:] = [d for d in dirs if d not in (".git", ".corbel", "node_modules", "target")]
        for f in files:
            if f.rsplit(".", 1)[-1] in LANG_EXTS:
                try:
                    total += os.path.getsize(os.path.join(root, f))
                except OSError:
                    pass
    return total


def run_index(repo_path, timeout=1800):
    start = time.perf_counter()
    proc = subprocess.run(
        [CORBEL, "index", str(repo_path)],
        capture_output=True, text=True, timeout=timeout,
    )
    elapsed = time.perf_counter() - start
    return elapsed, proc.stdout, proc.returncode


def cold_index_runs(repo_path, n=3, timeout=1800):
    times = []
    last_stdout = None
    for i in range(n):
        corbel_dir = Path(repo_path) / ".corbel"
        if corbel_dir.exists():
            shutil.rmtree(corbel_dir)
        elapsed, stdout, rc = run_index(repo_path, timeout=timeout)
        times.append(elapsed)
        last_stdout = stdout
        if rc != 0:
            return times, last_stdout, False
    return times, last_stdout, True


def incremental_index(repo_path, timeout=1800):
    rs_files = list(Path(repo_path).rglob("*.rs")) or list(Path(repo_path).rglob("*.ts"))
    target = None
    for f in rs_files:
        if ".corbel" not in f.parts:
            target = f
            break
    if target is None:
        return None
    original = target.read_text(encoding="utf-8", errors="replace")
    target.write_text(original + "\n// perf-bench incremental touch\n", encoding="utf-8")
    try:
        elapsed, stdout, rc = run_index(repo_path, timeout=timeout)
    finally:
        target.write_text(original, encoding="utf-8")
    return elapsed


def db_size_bytes(repo_path):
    p = Path(repo_path) / ".corbel" / "index.db"
    return p.stat().st_size if p.exists() else None


def parse_skipped(stdout):
    total_indexed = None
    skipped = 0
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("Indexed "):
            try:
                total_indexed = int(line.split()[1])
            except (IndexError, ValueError):
                pass
        if line.startswith("Skipped "):
            try:
                skipped = int(line.split()[1])
            except (IndexError, ValueError):
                pass
    return total_indexed, skipped


def sample_symbol_names(repo_path, n=100, seed=1234):
    db_path = Path(repo_path) / ".corbel" / "index.db"
    conn = sqlite3.connect(str(db_path))
    rows = conn.execute(
        "SELECT symbols.name, files.path, symbols.line FROM symbols "
        "JOIN files ON files.id = symbols.file_id "
        "WHERE symbols.kind IN ('function','method')"
    ).fetchall()
    conn.close()
    if not rows:
        return []
    rng = random.Random(seed)
    if len(rows) <= n:
        picks = rows * (n // len(rows) + 1)
    else:
        picks = rows
    return rng.sample(picks, n) if len(picks) >= n else picks[:n]


class CorbelClient:
    def __init__(self, binary_path, repo_root):
        self.binary_path = str(binary_path)
        self.repo_root = str(repo_root)
        self._id = 0
        self.proc = None

    def start(self):
        self.proc = subprocess.Popen(
            [self.binary_path, "serve", self.repo_root],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
            text=True, bufsize=1,
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
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
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
            raise RuntimeError("corbel serve produced no response")
        return json.loads(line)

    def call_tool(self, name, arguments):
        start = time.perf_counter()
        resp = self._request("tools/call", {"name": name, "arguments": arguments})
        elapsed = time.perf_counter() - start
        return resp, elapsed


def percentiles(samples, ps=(50, 95, 99)):
    if not samples:
        return {p: None for p in ps}
    s = sorted(samples)
    out = {}
    for p in ps:
        k = (len(s) - 1) * (p / 100)
        f = int(k)
        c = min(f + 1, len(s) - 1)
        if f == c:
            out[p] = s[f]
        else:
            out[p] = s[f] + (s[c] - s[f]) * (k - f)
    return out


def latency_bench(client, tool_name, arg_variants, n=100):
    samples = []
    errors = 0
    for i in range(n):
        args = arg_variants[i % len(arg_variants)]
        try:
            resp, elapsed = client.call_tool(tool_name, args)
            if "error" in resp:
                errors += 1
                continue
            samples.append(elapsed * 1000.0)
        except Exception:
            errors += 1
    return samples, errors


def peak_rss_kb_for_serve_session(binary_path, repo_root, fn):
    proc = subprocess.Popen(
        ["/usr/bin/time", "-v", binary_path, "serve", str(repo_root)],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, bufsize=1,
    )

    class WrappedClient:
        def __init__(self, proc):
            self.proc = proc
            self._id = 0

        def _notify(self, method):
            self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
            self.proc.stdin.flush()

        def _request(self, method, params=None):
            self._id += 1
            req = {"jsonrpc": "2.0", "id": self._id, "method": method}
            if params is not None:
                req["params"] = params
            self.proc.stdin.write(json.dumps(req) + "\n")
            self.proc.stdin.flush()
            line = self.proc.stdout.readline()
            return json.loads(line) if line else None

        def call_tool(self, name, arguments):
            start = time.perf_counter()
            resp = self._request("tools/call", {"name": name, "arguments": arguments})
            elapsed = time.perf_counter() - start
            return resp, elapsed

    client = WrappedClient(proc)
    client._request("initialize", {"protocolVersion": "2025-06-18"})
    client._notify("notifications/initialized")

    result = fn(client)

    try:
        proc.stdin.close()
    except Exception:
        pass
    try:
        _, stderr = proc.communicate(timeout=30)
    except subprocess.TimeoutExpired:
        proc.terminate()
        _, stderr = proc.communicate(timeout=15)

    peak_kb = None
    for line in stderr.splitlines():
        if "Maximum resident set size" in line:
            try:
                peak_kb = int(line.strip().split(":")[-1].strip())
            except ValueError:
                pass
    return result, peak_kb


def find_query_args(repo_path, n=100):
    db_path = Path(repo_path) / ".corbel" / "index.db"
    conn = sqlite3.connect(str(db_path))
    rows = conn.execute("SELECT name FROM symbols WHERE kind IN ('function','method') LIMIT 5000").fetchall()
    conn.close()
    substrings = set()
    for (name,) in rows:
        if len(name) >= 6:
            substrings.add(name[2:6].lower())
    substrings = list(substrings) or ["get"]
    rng = random.Random(99)
    return [{"query": s, "limit": 50} for s in rng.sample(substrings, min(n, len(substrings)))] or [{"query": "get", "limit": 50}]


def run_perf_suite(repo_name, repo_path, corbel_binary, cold_runs=3, latency_n=100):
    global CORBEL
    CORBEL = str(corbel_binary)
    repo_path = Path(repo_path)
    result = {"repo": repo_name, "path": str(repo_path)}

    result["source_size_bytes"] = source_size_bytes(repo_path)

    cold_times, stdout, ok = cold_index_runs(repo_path, n=cold_runs)
    result["cold_index_seconds"] = cold_times
    result["cold_index_ok"] = ok
    total_indexed, skipped = parse_skipped(stdout or "")
    result["files_indexed"] = total_indexed
    result["files_skipped"] = skipped
    result["skipped_ratio"] = (skipped / total_indexed) if total_indexed else None

    for line in (stdout or "").splitlines():
        line = line.strip()
        if line and line[0].isdigit() and "symbols" in line:
            parts = line.split(",")
            try:
                result["symbols"] = int(parts[0].split()[0])
                result["references"] = int(parts[1].split()[0])
            except Exception:
                pass

    result["db_size_bytes"] = db_size_bytes(repo_path)
    result["db_to_source_ratio"] = (
        result["db_size_bytes"] / result["source_size_bytes"]
        if result["db_size_bytes"] and result["source_size_bytes"] else None
    )

    result["incremental_index_seconds"] = incremental_index(repo_path)

    symbol_samples = sample_symbol_names(repo_path, n=latency_n)
    get_symbol_args = [{"name": s[0], "file": s[1], "line": s[2], "token_budget": 100000} for s in symbol_samples] or [{"name": "main"}]
    find_args = find_query_args(repo_path, n=latency_n)
    impact_args = [{"name": s[0], "file": s[1], "token_budget": 100000} for s in symbol_samples[:latency_n]] or [{"name": "main"}]

    def do_latency(client):
        get_symbol_samples, gs_errors = latency_bench(client, "get_symbol", get_symbol_args, n=latency_n)
        find_samples, find_errors = latency_bench(client, "find", find_args, n=latency_n)
        impact_samples, impact_errors = latency_bench(client, "impact", impact_args, n=min(30, latency_n))
        return {
            "get_symbol_ms": get_symbol_samples,
            "get_symbol_errors": gs_errors,
            "find_ms": find_samples,
            "find_errors": find_errors,
            "impact_ms": impact_samples,
            "impact_errors": impact_errors,
        }

    latencies, peak_rss_kb = peak_rss_kb_for_serve_session(CORBEL, repo_path, do_latency)
    result["peak_rss_kb"] = peak_rss_kb

    for key in ("get_symbol_ms", "find_ms", "impact_ms"):
        samples = latencies.get(key, [])
        p = percentiles(samples)
        result[key] = {"p50": p[50], "p95": p[95], "p99": p[99], "n": len(samples), "mean": statistics.mean(samples) if samples else None}
    result["get_symbol_errors"] = latencies.get("get_symbol_errors")
    result["find_errors"] = latencies.get("find_errors")
    result["impact_errors"] = latencies.get("impact_errors")

    return result


if __name__ == "__main__":
    repo_name = sys.argv[1]
    repo_path = sys.argv[2]
    corbel_binary = sys.argv[3]
    cold_runs = int(sys.argv[4]) if len(sys.argv) > 4 else 3
    latency_n = int(sys.argv[5]) if len(sys.argv) > 5 else 100
    result = run_perf_suite(repo_name, repo_path, corbel_binary, cold_runs=cold_runs, latency_n=latency_n)
    print(json.dumps(result, indent=2))
