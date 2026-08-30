import argparse
import json
import platform
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import metrics
import tool_adapters as ta
from report import render_json, render_markdown

HARNESS_DIR = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_DIR.parent.parent
DEFAULT_GOLDEN_DIR = REPO_ROOT / "benchmarks" / "golden"
DEFAULT_RESULTS_DIR = REPO_ROOT / "benchmarks" / "results"
DEFAULT_CORBEL_BINARY = REPO_ROOT / "target" / "release" / "corbel"


def load_golden_files(golden_dir):
    files = sorted(Path(golden_dir).glob("*.json"))
    golden_sets = []
    for f in files:
        with open(f, encoding="utf-8") as fh:
            golden_sets.append(json.load(fh))
    return golden_sets


def resolve_repo_path(golden_set):
    local_path = golden_set["local_path"]
    p = Path(local_path)
    if not p.is_absolute():
        p = REPO_ROOT / local_path
    return p.resolve()


def verify_commit(golden_set, repo_path):
    expected = golden_set["commit"]
    proc = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=repo_path, capture_output=True, text=True
    )
    actual = proc.stdout.strip()
    return actual, actual == expected


def task_ground_truth(task_value):
    if isinstance(task_value, list):
        return {"ambiguous": False, "ground_truth": task_value}
    return {"ambiguous": task_value.get("ambiguous", False), "ground_truth": task_value.get("ground_truth", [])}


def bare_name(qualified_name):
    for sep in ("::", "."):
        if sep in qualified_name:
            return qualified_name.rsplit(sep, 1)[-1]
    return qualified_name


def classify_miss(entry, item, corbel_raw=None, task_name=None):
    category = entry["category"]
    if item.get("qualified_path_call"):
        return "qualified_path_call_blind_spot"
    if task_name == "callers" and corbel_raw and "enclosing_symbol" in item:
        target_bare = bare_name(item["enclosing_symbol"])
        target_file = item.get("file")
        if any(
            r.get("file") == target_file and bare_name(r.get("enclosing_symbol", "")) == target_bare
            for r in corbel_raw
        ):
            return "unqualified_symbol_name"
    if category == "overload_ambiguous_name":
        return "name_collision_under_resolved"
    if category == "dynamic_dispatch":
        return "dynamic_dispatch_no_static_target"
    if category == "multi_hop":
        return "high_fan_in_under_collected"
    return "other_missed_reference"


def classify_extra(entry, extra_key, ground_truth_raw=None, task_name=None):
    category = entry["category"]
    if task_name == "callers" and ground_truth_raw and isinstance(extra_key, tuple):
        extra_name, extra_file = extra_key
        target_bare = bare_name(extra_name)
        if any(
            g.get("file") == extra_file and bare_name(g.get("enclosing_symbol", "")) == target_bare
            for g in ground_truth_raw
        ):
            return "unqualified_symbol_name"
    if category == "overload_ambiguous_name":
        return "name_collision_over_claimed"
    if category == "multi_hop":
        return "high_fan_in_over_claimed"
    return "other_spurious_match"


def _in_scope(file_rel, search_root):
    if search_root in (".", ""):
        return True
    fp = Path(file_rel)
    sr = Path(search_root)
    return fp == sr or sr in fp.parents


def run_caller_task(entry, repo_path, corbel_client, ctags_index, language, search_dir, search_root):
    sym = entry["symbol"]
    truth_info = task_ground_truth(entry["tasks"]["callers"])
    if truth_info["ambiguous"]:
        return None
    ground_truth = [metrics.caller_key(e) for e in truth_info["ground_truth"]]

    tool_runs = {}

    corbel_all, t, meta = ta.corbel_find_callers(corbel_client, sym["name"], sym["file"], sym["line"])
    corbel_raw = [c for c in corbel_all if _in_scope(c["file"], search_root)]
    meta = dict(meta, scoped_out_count=len(corbel_all) - len(corbel_raw))
    tool_runs["corbel"] = (corbel_raw, [metrics.caller_key(c) for c in corbel_raw], t, meta)

    rg_raw, t, meta = ta.ripgrep_find_callers(repo_path, sym["name"], sym["file"], sym["line"], language, search_dir)
    tool_runs["ripgrep"] = (rg_raw, [metrics.caller_key(c) for c in rg_raw], t, meta)

    grep_raw, t, meta = ta.grep_find_callers(repo_path, sym["name"], sym["file"], sym["line"], language, search_dir)
    tool_runs["grep"] = (grep_raw, [metrics.caller_key(c) for c in grep_raw], t, meta)

    ct_raw, t, meta = ta.ctags_find_callers(repo_path, sym["name"], sym["file"], sym["line"], language, ctags_index, search_dir)
    tool_runs["ctags"] = (ct_raw, [metrics.caller_key(c) for c in ct_raw], t, meta)

    return build_task_result(entry, "callers", ground_truth, truth_info["ground_truth"], tool_runs)


def run_callee_task(entry, repo_path, corbel_client, ctags_index, language, search_dir, search_root):
    sym = entry["symbol"]
    truth_info = task_ground_truth(entry["tasks"]["callees"])
    ground_truth = [metrics.callee_key(e["name"]) for e in truth_info["ground_truth"]]

    tool_runs = {}

    corbel_raw, t, meta = ta.corbel_find_callees(corbel_client, sym["name"], sym["file"], sym["line"])
    tool_runs["corbel"] = (corbel_raw, [metrics.callee_key(c) for c in corbel_raw], t, meta)

    rg_raw, t, meta = ta.ripgrep_find_callees(repo_path, sym["name"], sym["file"], sym["line"], language)
    tool_runs["ripgrep"] = (rg_raw, [metrics.callee_key(c) for c in rg_raw], t, meta)

    grep_raw, t, meta = ta.grep_find_callees(repo_path, sym["name"], sym["file"], sym["line"], language)
    tool_runs["grep"] = (grep_raw, [metrics.callee_key(c) for c in grep_raw], t, meta)

    ct_raw, t, meta = ta.ctags_find_callees(repo_path, sym["name"], sym["file"], sym["line"], language, ctags_index)
    tool_runs["ctags"] = (ct_raw, [metrics.callee_key(c) for c in ct_raw], t, meta)

    if truth_info["ambiguous"]:
        return {
            "entry_id": entry["id"],
            "symbol": sym,
            "category": entry["category"],
            "task": "callees",
            "ambiguous": True,
            "acceptable_answers": ground_truth,
            "tool_answers": {
                name: {"raw": raw, "keys": keys, "seconds": t, "meta": meta}
                for name, (raw, keys, t, meta) in tool_runs.items()
            },
        }

    return build_task_result(entry, "callees", ground_truth, truth_info["ground_truth"], tool_runs)


def build_task_result(entry, task_name, ground_truth_keys, ground_truth_raw, tool_runs):
    result = {
        "entry_id": entry["id"],
        "symbol": entry["symbol"],
        "category": entry["category"],
        "task": task_name,
        "ambiguous": False,
        "ground_truth": ground_truth_raw,
        "tools": {},
    }
    for tool_name, (raw, keys, elapsed, meta) in tool_runs.items():
        prf = metrics.score(keys, ground_truth_keys)
        matched, missing, extra = metrics.diff(keys, ground_truth_keys)
        failures = []
        if tool_name == "corbel":
            for m in missing:
                if task_name == "callers":
                    original = next((g for g in ground_truth_raw if metrics.caller_key(g) == m), {})
                elif task_name == "callees":
                    original = next((g for g in ground_truth_raw if metrics.callee_key(g["name"]) == m), {})
                else:
                    original = {}
                cause = classify_miss(entry, original, corbel_raw=raw, task_name=task_name)
                failures.append({"item": list(m) if isinstance(m, tuple) else m, "cause": cause})
            for e in extra:
                cause = classify_extra(entry, e, ground_truth_raw=ground_truth_raw, task_name=task_name)
                failures.append({"item": list(e) if isinstance(e, tuple) else e, "cause": cause})
        result["tools"][tool_name] = {
            "raw": raw,
            "seconds": elapsed,
            "meta": meta,
            "prf": {
                "tp": prf.tp, "fp": prf.fp, "fn": prf.fn,
                "precision": prf.precision, "recall": prf.recall, "f1": prf.f1,
            },
            "matched": [list(x) if isinstance(x, tuple) else x for x in matched],
            "missing": [list(x) if isinstance(x, tuple) else x for x in missing],
            "extra": [list(x) if isinstance(x, tuple) else x for x in extra],
            "failures": failures,
        }
    return result


def run_definition_task(entry, repo_path, corbel_client, ctags_index, language, search_dir, search_root):
    sym = entry["symbol"]
    ground_truth_raw = [{"file": sym["file"], "line": sym["line"]}]
    ground_truth = [metrics.definition_key(e) for e in ground_truth_raw]

    tool_runs = {}

    corbel_all, t, meta = ta.corbel_find_definition(corbel_client, sym["name"])
    corbel_raw = [c for c in corbel_all if _in_scope(c["file"], search_root)]
    meta = dict(meta, scoped_out_count=len(corbel_all) - len(corbel_raw))
    tool_runs["corbel"] = (corbel_raw, [metrics.definition_key(c) for c in corbel_raw], t, meta)

    rg_raw, t, meta = ta.ripgrep_find_definition(repo_path, sym["name"], language, search_dir)
    tool_runs["ripgrep"] = (rg_raw, [metrics.definition_key(c) for c in rg_raw], t, meta)

    grep_raw, t, meta = ta.grep_find_definition(repo_path, sym["name"], language, search_dir)
    tool_runs["grep"] = (grep_raw, [metrics.definition_key(c) for c in grep_raw], t, meta)

    ct_raw, t, meta = ta.ctags_find_definition(sym["name"], ctags_index)
    tool_runs["ctags"] = (ct_raw, [metrics.definition_key(c) for c in ct_raw], t, meta)

    return build_task_result(entry, "definition", ground_truth, ground_truth_raw, tool_runs)


def build_corbel_index(binary_path, repo_path):
    elapsed, stdout = ta.corbel_index(binary_path, repo_path)
    return elapsed, stdout


def collect_tool_versions():
    return {
        "corbel": ta.tool_version(str(DEFAULT_CORBEL_BINARY), "--version") if DEFAULT_CORBEL_BINARY.exists() else None,
        "grep": ta.tool_version("grep"),
        "ripgrep": ta.tool_version("rg"),
        "ctags": ta.tool_version("ctags"),
        "python": platform.python_version(),
        "os": f"{platform.system()} {platform.release()}",
    }


def run_repo(golden_set, corbel_binary):
    repo_path = resolve_repo_path(golden_set)
    if not repo_path.exists():
        raise FileNotFoundError(
            f"repo path {repo_path} does not exist for golden set {golden_set['repo']}; "
            f"clone it first (clone_url: {golden_set.get('clone_url')})"
        )

    actual_commit, commit_ok = verify_commit(golden_set, repo_path)

    language = golden_set["language"]
    search_root = golden_set.get("search_root", ".")
    search_dir = repo_path if search_root in (".", "") else (repo_path / search_root)

    index_time, index_stdout = build_corbel_index(corbel_binary, repo_path)

    corbel_client = ta.CorbelClient(corbel_binary, repo_path)
    corbel_client.start()

    ctags_build_start = time.perf_counter()
    ctags_index = ta.CtagsIndex(repo_path, language, search_dir)
    ctags_build_time = time.perf_counter() - ctags_build_start

    task_results = []
    try:
        for entry in golden_set["entries"]:
            tasks = entry["tasks"]
            if "callers" in tasks:
                r = run_caller_task(entry, repo_path, corbel_client, ctags_index, language, search_dir, search_root)
                if r:
                    task_results.append(r)
            if "callees" in tasks:
                r = run_callee_task(entry, repo_path, corbel_client, ctags_index, language, search_dir, search_root)
                if r:
                    task_results.append(r)
            r = run_definition_task(entry, repo_path, corbel_client, ctags_index, language, search_dir, search_root)
            task_results.append(r)
    finally:
        corbel_client.close()

    return {
        "repo": golden_set["repo"],
        "language": language,
        "expected_commit": golden_set["commit"],
        "actual_commit": actual_commit,
        "commit_matches": commit_ok,
        "search_root": search_root,
        "search_root_note": golden_set.get("search_root_note"),
        "corbel_index_seconds": index_time,
        "corbel_index_summary": index_stdout,
        "ctags_build_seconds": ctags_build_time,
        "task_results": task_results,
    }


def main():
    parser = argparse.ArgumentParser(description="corbel vs grep/ripgrep/ctags benchmark harness")
    parser.add_argument("--golden-dir", default=str(DEFAULT_GOLDEN_DIR))
    parser.add_argument("--results-dir", default=str(DEFAULT_RESULTS_DIR))
    parser.add_argument("--corbel-binary", default=str(DEFAULT_CORBEL_BINARY))
    parser.add_argument("--repo", action="append", default=None, help="restrict to one repo name (repeatable)")
    args = parser.parse_args()

    corbel_binary = Path(args.corbel_binary)
    if not corbel_binary.exists():
        print(f"error: corbel binary not found at {corbel_binary}; run `cargo build --release -p corbel` first", file=sys.stderr)
        sys.exit(1)

    golden_sets = load_golden_files(args.golden_dir)
    if args.repo:
        golden_sets = [g for g in golden_sets if g["repo"] in args.repo]
    if not golden_sets:
        print("error: no golden set files matched", file=sys.stderr)
        sys.exit(1)

    started_at = datetime.now(timezone.utc).isoformat()
    tool_versions = collect_tool_versions()
    tool_versions["corbel"] = ta.tool_version(str(corbel_binary), "--version")

    repo_results = []
    for golden_set in golden_sets:
        print(f"running benchmark for {golden_set['repo']}...", file=sys.stderr)
        repo_results.append(run_repo(golden_set, corbel_binary))

    run_report = {
        "started_at": started_at,
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "tool_versions": tool_versions,
        "repos": repo_results,
    }

    results_dir = Path(args.results_dir)
    results_dir.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")

    json_path = results_dir / f"benchmark-{timestamp}.json"
    md_path = results_dir / f"benchmark-{timestamp}.md"

    with open(json_path, "w", encoding="utf-8") as f:
        f.write(render_json(run_report))
    with open(md_path, "w", encoding="utf-8") as f:
        f.write(render_markdown(run_report))

    latest_json = results_dir / "latest.json"
    latest_md = results_dir / "latest.md"
    latest_json.write_text(render_json(run_report), encoding="utf-8")
    latest_md.write_text(render_markdown(run_report), encoding="utf-8")

    print(f"wrote {json_path}", file=sys.stderr)
    print(f"wrote {md_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
