import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import run_benchmark
import tool_adapters as ta

HARNESS_DIR = Path(__file__).resolve().parent
REPO_ROOT = HARNESS_DIR.parent.parent


def _find_corbel_binary():
    for profile in ("release", "debug"):
        candidate = REPO_ROOT / "target" / profile / "corbel"
        if candidate.exists():
            return candidate
    return None


def _write_hot_function_fixture(repo_dir, caller_count):
    lines = ["pub fn hot() {}"]
    for i in range(caller_count):
        lines.append(f"pub fn caller_{i}() {{\n    hot();\n}}")
    (Path(repo_dir) / "hot.rs").write_text("\n".join(lines) + "\n", encoding="utf-8")


@unittest.skipIf(_find_corbel_binary() is None, "corbel binary not built; run cargo build -p corbel")
class CorbelAdapterTruncationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary_path = _find_corbel_binary()
        cls.repo_dir = tempfile.mkdtemp(prefix="corbel-bench-adapter-test-")
        _write_hot_function_fixture(cls.repo_dir, caller_count=12)
        subprocess.run(
            ["git", "init"], cwd=cls.repo_dir, capture_output=True, check=True
        )
        ta.corbel_index(cls.binary_path, cls.repo_dir)
        cls.client = ta.CorbelClient(cls.binary_path, cls.repo_dir)
        cls.client.start()

    @classmethod
    def tearDownClass(cls):
        cls.client.close()
        shutil.rmtree(cls.repo_dir, ignore_errors=True)

    def test_default_benchmark_budget_does_not_truncate(self):
        callers, _elapsed, meta = ta.corbel_find_callers(
            self.client, "hot", "hot.rs", 1
        )
        self.assertEqual(len(callers), 12)
        self.assertFalse(meta["truncated"])
        self.assertEqual(meta["truncated_count"], 0)

    def test_tiny_budget_truncates_and_is_reported(self):
        callers, _elapsed, meta = ta.corbel_find_callers(
            self.client, "hot", "hot.rs", 1, token_budget=1
        )
        self.assertLess(len(callers), 12)
        self.assertTrue(meta["truncated"])
        self.assertGreater(meta["truncated_count"], 0)

    def test_tiny_budget_truncation_surfaces_through_callees_adapter_too(self):
        callees, _elapsed, meta = ta.corbel_find_callees(
            self.client, "caller_0", "hot.rs", 2, token_budget=1
        )
        self.assertEqual(callees, [])
        self.assertTrue(meta["truncated"])
        self.assertGreater(meta["truncated_count"], 0)

    def test_get_symbol_client_wrapper_passes_token_budget_through(self):
        payload = self.client.get_symbol("hot", file="hot.rs", line=1, token_budget=1)
        self.assertTrue(payload["found"])
        self.assertTrue(payload["results"][0]["truncated"])


class TypeScriptAdapterTests(unittest.TestCase):
    """Regression coverage for TS/JS language support in tool_adapters.py /
    enclosing.py. Added alongside the fix for the rust/python-only binary
    that made every grep/ripgrep/ctags result on TS golden sets come back
    0/0/0."""

    @classmethod
    def setUpClass(cls):
        cls.repo_dir = Path(tempfile.mkdtemp(prefix="corbel-bench-ts-test-"))
        (cls.repo_dir / "node_modules").mkdir()
        (cls.repo_dir / "node_modules" / "junk.ts").write_text(
            "export function shouldBeExcluded() {}\n", encoding="utf-8"
        )
        (cls.repo_dir / "fixture.ts").write_text(
            "export function outer(): void {\n"
            "  const inner: any = function () {};\n"
            "  helper(inner);\n"
            "}\n"
            "\n"
            "function helper(x: any): void {}\n"
            "\n"
            "class Foo {\n"
            "  bar(): void {\n"
            "    baz();\n"
            "  }\n"
            "}\n"
            "\n"
            "function baz(): void {}\n",
            encoding="utf-8",
        )
        (cls.repo_dir / "fixture.tsx").write_text(
            "export function widget(): void {\n"
            "  baz();\n"
            "}\n",
            encoding="utf-8",
        )

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.repo_dir, ignore_errors=True)

    def test_keywords_and_exclude_dirs_are_ts_specific(self):
        self.assertIn("function", ta._keywords_for("typescript"))
        self.assertIn("async", ta._keywords_for("typescript"))
        self.assertIn("node_modules", ta._exclude_dirs("typescript"))
        self.assertIn("dist", ta._exclude_dirs("typescript"))
        self.assertIn("build", ta._exclude_dirs("typescript"))

    def test_ripgrep_find_definition_matches_function_declaration(self):
        results, _elapsed, _meta = ta.ripgrep_find_definition(self.repo_dir, "helper", "typescript")
        self.assertEqual(results, [{"file": "fixture.ts", "line": 6}])

    def test_grep_find_definition_matches_function_declaration(self):
        results, _elapsed, _meta = ta.grep_find_definition(self.repo_dir, "helper", "typescript")
        self.assertEqual(results, [{"file": "fixture.ts", "line": 6}])

    def test_definition_search_excludes_node_modules(self):
        results, _elapsed, _meta = ta.ripgrep_find_definition(self.repo_dir, "shouldBeExcluded", "typescript")
        self.assertEqual(results, [])

    def test_ripgrep_find_definition_covers_both_ts_and_tsx(self):
        # baz() is defined in fixture.ts but called from fixture.tsx; a
        # definition search for baz must still only find the .ts definition,
        # proving both extensions are scanned without cross-matching calls
        # as definitions.
        results, _elapsed, _meta = ta.ripgrep_find_definition(self.repo_dir, "baz", "typescript")
        self.assertEqual(results, [{"file": "fixture.ts", "line": 14}])

    def test_ripgrep_find_callers_resolves_class_method_enclosing_symbol(self):
        callers, _elapsed, _meta = ta.ripgrep_find_callers(
            self.repo_dir, "baz", "fixture.ts", 14, "typescript"
        )
        by_file = {(c["file"], c["line"]): c["enclosing_symbol"] for c in callers}
        self.assertEqual(by_file[("fixture.ts", 10)], "Foo.bar")
        self.assertEqual(by_file[("fixture.tsx", 2)], "widget")

    def test_grep_find_callers_resolves_class_method_enclosing_symbol(self):
        callers, _elapsed, _meta = ta.grep_find_callers(
            self.repo_dir, "baz", "fixture.ts", 14, "typescript"
        )
        by_file = {(c["file"], c["line"]): c["enclosing_symbol"] for c in callers}
        self.assertEqual(by_file[("fixture.ts", 10)], "Foo.bar")

    def test_nested_local_closure_does_not_shadow_outer_enclosing_symbol(self):
        # `const inner = function () {}` at line 2 is a local closure nested
        # inside `outer`; the call to helper() at line 3 must still resolve
        # to `outer`, not to `inner` (see TS_JS_LIMITATIONS.md).
        callers, _elapsed, _meta = ta.ripgrep_find_callers(
            self.repo_dir, "helper", "fixture.ts", 6, "typescript"
        )
        self.assertEqual(len(callers), 1)
        self.assertEqual(callers[0]["enclosing_symbol"], "outer")


class CollectTruncatedCasesTests(unittest.TestCase):
    def test_detects_truncation_in_tools_shaped_task_result(self):
        task_results = [
            {
                "entry_id": "repo-01",
                "symbol": {"name": "hot"},
                "task": "callers",
                "tools": {
                    "corbel": {"meta": {"truncated": True, "truncated_count": 7}},
                    "ripgrep": {"meta": {}},
                },
            }
        ]
        cases = run_benchmark.collect_truncated_cases("repo", task_results)
        self.assertEqual(
            cases,
            [
                {
                    "repo": "repo",
                    "entry_id": "repo-01",
                    "symbol": "hot",
                    "task": "callers",
                    "truncated_count": 7,
                }
            ],
        )

    def test_detects_truncation_in_ambiguous_tool_answers_shaped_task_result(self):
        task_results = [
            {
                "entry_id": "repo-02",
                "symbol": {"name": "dispatch"},
                "task": "callees",
                "ambiguous": True,
                "tool_answers": {
                    "corbel": {"meta": {"truncated": True, "truncated_count": 3}},
                },
            }
        ]
        cases = run_benchmark.collect_truncated_cases("repo", task_results)
        self.assertEqual(len(cases), 1)
        self.assertEqual(cases[0]["truncated_count"], 3)

    def test_does_not_report_untruncated_task_results(self):
        task_results = [
            {
                "entry_id": "repo-03",
                "symbol": {"name": "quiet"},
                "task": "callers",
                "tools": {
                    "corbel": {"meta": {"truncated": False, "truncated_count": 0}},
                },
            }
        ]
        self.assertEqual(run_benchmark.collect_truncated_cases("repo", task_results), [])

    def test_missing_meta_is_treated_as_not_truncated(self):
        task_results = [
            {
                "entry_id": "repo-04",
                "symbol": {"name": "no_meta"},
                "task": "definition",
                "tools": {"corbel": {"meta": {}}},
            }
        ]
        self.assertEqual(run_benchmark.collect_truncated_cases("repo", task_results), [])


if __name__ == "__main__":
    unittest.main()
