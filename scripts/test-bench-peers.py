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

    def test_native_duckdb_fresh_samples_validate_all_fields(self):
        data = bench.dataset(41)
        peer = bench.NativeInsert(data, SimpleNamespace(native_timestamp_ms=123456), "linbench_contract", "duckdb")
        try:
            timings, iterations, rows = peer.measure_insert(3)
            self.assertEqual(len(timings), 3)
            self.assertTrue(all(value > 0 for value in timings))
            self.assertEqual(iterations, 1)
            bench.validate(rows, peer.want, "duckdb", "insert_native")
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

    def test_tied_times_are_not_a_win(self):
        observations = [{"case": "point_get/" + engine, "repetition": i, "samples_ns": [100]} for engine in ["lin", "sqlite"] for i in range(2)]
        result, run = self.run_gate(observations)
        self.assertEqual(run["status"], "complete")
        self.assertEqual(result, 3)

    def test_duplicate_process_ids_are_incomplete(self):
        observations = [{"case": "point_get/" + engine, "repetition": 0, "samples_ns": [100 if engine == "lin" else 200]} for engine in ["lin", "sqlite"] for _ in range(2)]
        result, run = self.run_gate(observations)
        self.assertEqual(result, 2)
        self.assertEqual(run["status"], "incomplete")

    def test_invalid_timing_samples_are_incomplete(self):
        for sample in [float("nan"), float("inf"), -1, 0]:
            with self.subTest(sample=sample):
                observations = [{"case": "point_get/" + engine, "repetition": i, "samples_ns": [sample if engine == "lin" else 200]} for engine in ["lin", "sqlite"] for i in range(2)]
                result, run = self.run_gate(observations)
                self.assertEqual(result, 2)
                self.assertEqual(run["status"], "incomplete")

    def test_empty_timing_samples_are_incomplete(self):
        observations = [{"case": "point_get/" + engine, "repetition": i, "samples_ns": []} for engine in ["lin", "sqlite"] for i in range(2)]
        result, run = self.run_gate(observations)
        self.assertEqual(result, 2)
        self.assertEqual(run["status"], "incomplete")

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

    def test_native_mssql_bulk_transaction_and_owned_cleanup(self):
        import pytds
        data = bench.dataset(41)
        first = data[0][0]
        data[0][0] = (*first[:3], "ЁЖ SQL 🦔", "quote ' comma, tab\t newline\n")
        want = [[d[0], d[1], d[2], d[3], 123456, d[4]] for d in data[0]]
        for mode in ["ok", "bulk_failure", "wrong_readback", "create_failure", "rollback_failure"]:
            with self.subTest(mode=mode):
                events = []
                copies = []
                class Cursor:
                    def __enter__(self): return self
                    def __exit__(self, *args): events.append("cursor_close")
                    def execute(self, sql):
                        self.last = sql
                        events.append(sql)
                        if mode == "create_failure" and sql.startswith("CREATE TABLE"):
                            raise RuntimeError("injected existing fixture")
                        if mode == "rollback_failure" and "ROLLBACK TRANSACTION" in sql:
                            raise RuntimeError("injected rollback failure")
                    def fetchone(self):
                        return ("16.test",) if "SERVERPROPERTY" in self.last else ("FULL", "DISABLED")
                    def copy_to(self, **kwargs):
                        copies.append(kwargs)
                        events.append("COPY")
                        if mode == "bulk_failure": raise RuntimeError("injected bulk failure")
                    def fetchall(self):
                        return want[:-1] + [want[0]] if mode == "wrong_readback" else want
                class Connection:
                    def cursor(self): return Cursor()
                    def close(self): events.append("connection_close")
                with patch.dict(os.environ, {"LIN_BENCH_MSSQL_HOST": "test-only", "LIN_BENCH_MSSQL_USER": "u", "LIN_BENCH_MSSQL_PASSWORD": "p", "LIN_BENCH_MSSQL_DATABASE": "testdb"}):
                    with patch.object(pytds, "connect", return_value=Connection()) as connect:
                        peer = bench.NativeInsert(data, SimpleNamespace(native_timestamp_ms=123456), "linbench_mssql_owned", "mssql")
                        try:
                            if mode == "ok":
                                ns, iterations, got = peer.measure_insert(2)
                                self.assertEqual(len(ns), 2)
                                self.assertEqual(iterations, 1)
                                self.assertEqual(got, want)
                            else:
                                with self.assertRaises((RuntimeError, AssertionError)):
                                    peer.measure_insert(1)
                            self.assertTrue(connect.call_args.kwargs["autocommit"])
                            self.assertEqual(peer.contract["delayed_durability"], "DISABLED")
                        finally: peer.close()
                creates = [e for e in events if e.startswith("CREATE TABLE")]
                drops = [e for e in events if e.startswith("DROP TABLE")]
                self.assertEqual(len(drops), 0 if mode == "create_failure" else len(creates))
                for drop in drops: self.assertIn("linbench_mssql_owned_docs_", drop)
                self.assertIn("connection_close", events)
                if copies:
                    for copy in copies:
                        self.assertEqual(copy["data"], want)
                        self.assertTrue(copy["check_constraints"])
                        self.assertTrue(copy["fire_triggers"])
                        self.assertEqual(copy["schema"], "dbo")
                        self.assertEqual([c.column_name for c in copy["columns"]], ["id", "uri", "wing", "title", "ts", "body"])
                        self.assertEqual([c.type.get_declaration() for c in copy["columns"]], ["NVARCHAR(64)", "NVARCHAR(128)", "NVARCHAR(16)", "NVARCHAR(128)", "BIGINT", "NVARCHAR(128)"])
                    self.assertIn("BEGIN TRANSACTION", events)
                    if mode == "bulk_failure": self.assertNotIn("COMMIT TRANSACTION", events)
                    else: self.assertIn("COMMIT TRANSACTION", events)
                    self.assertTrue(any("ROLLBACK TRANSACTION" in e for e in events))

    def test_kusto_cleanup_attempts_every_owned_table_after_drop_failure(self):
        peer = object.__new__(bench.Kusto)
        peer.created = ["linbench_owned_docs", "linbench_owned_users", "linbench_owned_orders"]
        attempted = []
        closed = []
        peer.connection = SimpleNamespace(close=lambda: closed.append(True))
        def execute(query, management=False):
            self.assertTrue(management)
            attempted.append(query)
            if "orders" in query or "docs" in query:
                raise RuntimeError("injected drop failure")
        peer.execute = execute
        with self.assertRaises(RuntimeError):
            peer.close()
        self.assertEqual(attempted, [f".drop table {name} ifexists" for name in reversed(peer.created)])
        self.assertEqual(closed, [True])

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
