#!/usr/bin/env python3
"""Correctness and incomplete-run gates; no live server credentials needed."""
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("bench_peers", Path(__file__).with_name("bench-peers.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class BenchmarkContractTests(unittest.TestCase):
    def test_sqlite_returns_every_expected_value(self):
        data = bench.dataset(41)
        peer = bench.Sql(data, None, "linbench_contract", "sqlite")
        try:
            for case in bench.CASES:
                bench.validate(peer.query(case), bench.expected(data, case), "sqlite", case)
        finally:
            peer.close()

    def test_native_sqlite_fresh_samples_validate_all_fields(self):
        data = bench.dataset(41)
        peer = bench.NativeInsert(data, SimpleNamespace(native_timestamp_ms=123456), "linbench_contract", "sqlite")
        try:
            timings, iterations, rows = peer.measure_insert(3)
            self.assertEqual(len(timings), 3)
            self.assertTrue(all(value > 0 for value in timings))
            self.assertEqual(iterations, 1)
            bench.validate(rows, peer.want, "sqlite", "insert_native")
            self.assertTrue(all(row[4] == 123456 for row in rows))
        finally:
            peer.close()

    def test_same_count_with_wrong_values_or_duplicates_is_rejected(self):
        for wrong in [[["b"], ["a"]], [["a"], ["a"]]]:
            with self.assertRaises(AssertionError):
                bench.validate(wrong, [["a"], ["c"]], "test", "rows")
        bench.validate([["b"], ["a"]], [["a"], ["b"]], "test", "rows")

    def run_gate(self, observations, require_wins=True):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(cases=["point_get"], engines=["lin", "sqlite"],
                repeats=2, output=Path(directory), require_wins=require_wins)
            run = {"observations": observations, "errors": [], "medians_ns": {},
                "rows": 41, "repeats": 2, "host": "test", "cases": args.cases,
                "engines": args.engines, "versions": {}}
            result = bench.finish(args, run)
            return result, run

    def test_gate_aggregates_processes_before_comparing(self):
        observations = [
            {"case": "point_get/lin", "repetition": 0, "samples_ns": [100, 100, 100]},
            {"case": "point_get/lin", "repetition": 1, "samples_ns": [1000]},
            {"case": "point_get/sqlite", "repetition": 0, "samples_ns": [400]},
            {"case": "point_get/sqlite", "repetition": 1, "samples_ns": [400]},
        ]
        result, run = self.run_gate(observations)
        self.assertEqual(run["medians_ns"]["point_get/lin"], 550)
        self.assertEqual(result, 3, "a measured loss must fail the wins gate")

    def test_missing_peer_is_incomplete(self):
        observations = [{"case": "point_get/lin", "repetition": i, "samples_ns": [100]} for i in range(2)]
        result, run = self.run_gate(observations)
        self.assertEqual(result, 2)
        self.assertEqual(run["status"], "incomplete")

    def test_independent_process_repetitions_produce_a_complete_report(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "run"
            command = [sys.executable, str(Path(__file__).with_name("bench-peers.py")),
                "--engines", "sqlite", "--cases", "point_get", "--rows", "41",
                "--samples", "2", "--sample-ms", "0.1", "--repeats", "2", "--output", str(output)]
            completed = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            run = json.loads((output / "run.json").read_text())
            self.assertEqual(run["status"], "complete")
            self.assertEqual({o["repetition"] for o in run["observations"]}, {0, 1})
            self.assertTrue((output / "process-1/run.json").is_file())
            self.assertTrue((output / "process-2/run.json").is_file())

    def test_unsupported_native_peer_is_incomplete(self):
        with tempfile.TemporaryDirectory() as directory:
            command = [sys.executable, str(Path(__file__).with_name("bench-peers.py")),
                "--engines", "pandas", "--cases", "insert_native", "--rows", "41",
                "--samples", "1", "--repeats", "1", "--output", str(Path(directory) / "run")]
            completed = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(completed.returncode, 2, completed.stderr)
            run = json.loads((Path(directory) / "run/run.json").read_text())
            self.assertEqual(run["status"], "incomplete")
            self.assertEqual(run["medians_ns"], {})
            self.assertTrue(run["errors"])

    def test_kusto_partial_query_failure_is_not_a_success(self):
        def payload(severity, code):
            return {"Tables": [
                {"TableName": "PrimaryResult", "Rows": [[7]]},
                {"TableName": "QueryStatus", "Columns": [{"ColumnName": "Severity"}, {"ColumnName": "StatusCode"}], "Rows": [[severity, code]]},
            ]}
        peer = object.__new__(bench.Kusto)
        peer.database = "test"
        peer.base_path = ""
        class Connection:
            result = None
            def request(self, *args):
                pass
            def getresponse(self):
                result = self.result
                return SimpleNamespace(status=200, read=lambda: json.dumps(result).encode())
        peer.connection = Connection()
        with patch.dict(os.environ, {"LIN_BENCH_KUSTO_TOKEN": ""}):
            peer.connection.result = payload(4, 0)
            self.assertEqual(peer.execute("print 7"), [[7]])
            for severity, code in [(2, 0), (4, 500)]:
                peer.connection.result = payload(severity, code)
                with self.assertRaises(RuntimeError):
                    peer.execute("print 7")


if __name__ == "__main__":
    unittest.main()
