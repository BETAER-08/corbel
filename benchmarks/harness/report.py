import json

import metrics

TOOLS = ["corbel", "grep", "ripgrep", "ripgrep+ctags"]


def render_json(run_report):
    return json.dumps(run_report, indent=2, sort_keys=False)


def _fmt(x, digits=3):
    if x is None:
        return "n/a"
    return f"{x:.{digits}f}"


def _aggregate_per_tool(task_results):
    per_tool = {t: [] for t in TOOLS}
    for tr in task_results:
        if tr.get("ambiguous"):
            continue
        for tool_name, data in tr["tools"].items():
            prf = metrics.PRF(
                tp=data["prf"]["tp"], fp=data["prf"]["fp"], fn=data["prf"]["fn"],
                precision=data["prf"]["precision"], recall=data["prf"]["recall"], f1=data["prf"]["f1"],
            )
            per_tool[tool_name].append((prf, data["seconds"]))
    summary = {}
    for tool_name, items in per_tool.items():
        if not items:
            summary[tool_name] = None
            continue
        prfs = [i[0] for i in items]
        agg = metrics.aggregate(prfs)
        total_time = sum(i[1] for i in items)
        summary[tool_name] = {
            "precision": agg.precision, "recall": agg.recall, "f1": agg.f1,
            "tp": agg.tp, "fp": agg.fp, "fn": agg.fn,
            "total_seconds": total_time,
            "task_count": len(items),
        }
    return summary


def _failure_causes(task_results):
    causes = {}
    for tr in task_results:
        if tr.get("ambiguous"):
            continue
        corbel_data = tr["tools"].get("corbel")
        if not corbel_data:
            continue
        for f in corbel_data["failures"]:
            causes.setdefault(f["cause"], []).append(
                {"entry_id": tr["entry_id"], "task": tr["task"], "item": f["item"]}
            )
    return causes


def render_markdown(run_report):
    lines = []
    lines.append("# corbel vs grep / ripgrep / ripgrep+ctags benchmark")
    lines.append("")
    lines.append(
        "`ripgrep+ctags` is a hybrid, not plain ctags: call-site discovery comes "
        "from ripgrep (or grep if ripgrep is unavailable), and ctags supplies only "
        "the enclosing-scope/end-line lookup for those hits (definition lookup is "
        "the one task answered by ctags alone). Plain ctags has no call-site index "
        "and could not attempt the callers task at all, which is why this harness "
        "measures the hybrid a real user would reach for instead of a strawman "
        "zero score."
    )
    lines.append("")

    if not run_report.get("callees_task_included", True):
        lines.append(
            "**The callees (T2) task was skipped in this run "
            f"({run_report.get('callees_task_exclusion_reason', '')}). "
            "No callees rows appear in the tables below. Pass `--include-callees` "
            "to include it.**"
        )
        lines.append("")

    all_truncated_cases = run_report.get("truncated_cases", [])
    if all_truncated_cases:
        lines.append(
            f"**WARNING: corbel's response was truncated in {len(all_truncated_cases)} "
            f"case(s) despite a {run_report.get('benchmark_token_budget')}-token budget. "
            "Precision/recall for the affected entries may be understated — see "
            "\"Truncated cases\" in each repository section below before trusting "
            "any recall number in this report.**"
        )
        lines.append("")

    lines.append(f"Run started: {run_report['started_at']}")
    lines.append(f"Run finished: {run_report['finished_at']}")
    lines.append("")
    lines.append("## Tool versions")
    lines.append("")
    lines.append("| Tool | Version |")
    lines.append("| --- | --- |")
    for name, version in run_report["tool_versions"].items():
        lines.append(f"| {name} | {version if version else 'n/a'} |")
    lines.append("")
    lines.append(
        f"Accuracy runs use `BENCHMARK_TOKEN_BUDGET = "
        f"{run_report.get('benchmark_token_budget')}` for every corbel `get_symbol` "
        "call, not corbel's own built-in default, so truncation cannot silently "
        "depress recall."
    )
    lines.append("")
    if run_report.get("benchmark_token_budget_rationale"):
        lines.append(f"Rationale: {run_report['benchmark_token_budget_rationale']}")
        lines.append("")

    for repo in run_report["repos"]:
        lines.append(f"## Repository: {repo['repo']} ({repo['language']})")
        lines.append("")
        lines.append(f"- Expected commit: `{repo['expected_commit']}`")
        lines.append(f"- Actual commit at run time: `{repo['actual_commit']}`")
        if repo["commit_matches"]:
            lines.append("- Commit match: yes")
        else:
            mismatch = repo.get("commit_mismatch_detail") or {}
            lines.append("- Commit match: **NO - results not reproducible against pinned commit**")
            source_changed = mismatch.get("source_changed")
            if source_changed is True:
                lines.append(f"  - **Source files changed since pin.** {mismatch.get('detail')}")
            elif source_changed is False:
                lines.append(f"  - No source changes detected. {mismatch.get('detail')}")
            elif mismatch:
                lines.append(f"  - **Could not determine drift type.** {mismatch.get('detail')}")
        lines.append(f"- Search scope: `{repo['search_root']}` (all four tools, including corbel's repo-wide index results, are restricted to this path before scoring)")
        if repo.get("search_root_note"):
            lines.append(f"- Search scope rationale: {repo['search_root_note']}")
        lines.append(f"- corbel index time: {_fmt(repo['corbel_index_seconds'])}s")
        lines.append(f"- ctags build time: {_fmt(repo['ctags_build_seconds'])}s")
        lines.append("")

        lines.append("### Truncated cases")
        lines.append("")
        repo_truncated = repo.get("truncated_cases", [])
        if not repo_truncated:
            lines.append(
                f"None. Every corbel call in this repository fit within the "
                f"{run_report.get('benchmark_token_budget')}-token accuracy budget; "
                "no precision/recall number below was affected by truncation."
            )
            lines.append("")
        else:
            lines.append(
                "**These cases were cut by the token budget, not genuinely "
                "unresolved by corbel. This is not a correctness failure — it is "
                "reported separately from the failure-cause table below.**"
            )
            lines.append("")
            lines.append("| Entry | Symbol | Task | Entries cut |")
            lines.append("| --- | --- | --- | --- |")
            for case in repo_truncated:
                lines.append(
                    f"| {case['entry_id']} | {case['symbol']} | {case['task']} | "
                    f"{case['truncated_count']} |"
                )
            lines.append("")

        summary = _aggregate_per_tool(repo["task_results"])
        lines.append("### Aggregate precision / recall / F1 / time (non-ambiguous tasks only)")
        lines.append("")
        lines.append("| Tool | Precision | Recall | F1 | TP | FP | FN | Total query time (s) | Tasks scored |")
        lines.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- |")
        for tool_name in TOOLS:
            s = summary.get(tool_name)
            if s is None:
                lines.append(f"| {tool_name} | n/a | n/a | n/a | 0 | 0 | 0 | n/a | 0 |")
                continue
            lines.append(
                f"| {tool_name} | {_fmt(s['precision'])} | {_fmt(s['recall'])} | {_fmt(s['f1'])} | "
                f"{s['tp']} | {s['fp']} | {s['fn']} | {_fmt(s['total_seconds'])} | {s['task_count']} |"
            )
        lines.append("")

        lines.append("### Per-task results")
        lines.append("")
        for tr in repo["task_results"]:
            sym = tr["symbol"]
            lines.append(f"#### {tr['entry_id']} — `{sym['name']}` ({tr['task']}) — category: `{tr['category']}`")
            lines.append("")
            lines.append(f"Definition: `{sym['file']}:{sym['line']}`")
            lines.append("")
            if tr.get("ambiguous"):
                lines.append("**Ambiguous ground truth (dynamic dispatch / duck typing) — excluded from scored aggregates.**")
                lines.append("")
                lines.append(f"Acceptable answers (any one is a plausible runtime target): `{tr['acceptable_answers']}`")
                lines.append("")
                lines.append("| Tool | Reported answer | Time (s) |")
                lines.append("| --- | --- | --- |")
                for tool_name in TOOLS:
                    ans = tr["tool_answers"].get(tool_name)
                    if ans is None:
                        lines.append(f"| {tool_name} | n/a | n/a |")
                        continue
                    lines.append(f"| {tool_name} | {ans['keys']} | {_fmt(ans['seconds'])} |")
                lines.append("")
                continue

            lines.append(f"Ground truth: `{tr['ground_truth']}`")
            lines.append("")
            lines.append("| Tool | Precision | Recall | F1 | TP | FP | FN | Time (s) |")
            lines.append("| --- | --- | --- | --- | --- | --- | --- | --- |")
            for tool_name in TOOLS:
                data = tr["tools"].get(tool_name)
                if data is None:
                    lines.append(f"| {tool_name} | n/a | n/a | n/a | 0 | 0 | 0 | n/a |")
                    continue
                prf = data["prf"]
                lines.append(
                    f"| {tool_name} | {_fmt(prf['precision'])} | {_fmt(prf['recall'])} | {_fmt(prf['f1'])} | "
                    f"{prf['tp']} | {prf['fp']} | {prf['fn']} | {_fmt(data['seconds'])} |"
                )
            lines.append("")
            corbel_data = tr["tools"].get("corbel")
            if corbel_data and (corbel_data["missing"] or corbel_data["extra"]):
                if corbel_data["missing"]:
                    lines.append(f"- corbel missed: `{corbel_data['missing']}`")
                if corbel_data["extra"]:
                    lines.append(f"- corbel spurious: `{corbel_data['extra']}`")
                lines.append("")

        lines.append("### corbel failure causes")
        lines.append("")
        causes = _failure_causes(repo["task_results"])
        if not causes:
            lines.append("No scored failures for corbel in this repository.")
            lines.append("")
        else:
            lines.append("| Cause | Count | Examples (entry:task -> item) |")
            lines.append("| --- | --- | --- |")
            for cause, items in sorted(causes.items(), key=lambda kv: -len(kv[1])):
                examples = "; ".join(f"{i['entry_id']}:{i['task']} -> {i['item']}" for i in items[:5])
                lines.append(f"| {cause} | {len(items)} | {examples} |")
            lines.append("")

    return "\n".join(lines) + "\n"
