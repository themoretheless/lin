#!/usr/bin/env python3
"""Validated read API comparisons. See docs/benchmark-contract.md for scope."""
from __future__ import annotations

import argparse
import csv
import http.client
import importlib.metadata
import io
import json
import os
import platform
import statistics
import subprocess
import sys
import time
import urllib.parse
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINES = ["lin", "sqlite", "duckdb", "postgres", "mysql", "mongo", "mssql", "kusto", "pandas"]
CASES = ["point_get", "filter_eq", "text_substr", "materialize", "join_inner", "join_filter"]


def client_versions():
    versions = {}
    for package in ["pandas", "duckdb", "pymongo", "psycopg", "PyMySQL", "python-tds"]:
        try:
            versions[package] = importlib.metadata.version(package)
        except importlib.metadata.PackageNotFoundError:
            versions[package] = None
    return versions


def dataset(n):
    docs = [(f"d-{i}", f"bench://{i}", "rag" if i % 2 == 0 else "sys",
             f"doc {i} wal note" if i % 10 == 0 else f"doc {i} plain", f"body {i}") for i in range(n)]
    users = [(f"u-{i}", f"u{i}@bench.local") for i in range(max(1, n // 10))]
    orders = [(f"o-{i}", f"u-{i % len(users)}", i % 200) for i in range(n)]
    return docs, users, orders


def expected(data, case):
    docs, users, orders = data
    emails = dict(users)
    return {
        "point_get": lambda: [[docs[20][0], docs[20][3]]],
        "filter_eq": lambda: [[sum(d[2] == "rag" for d in docs)]],
        "text_substr": lambda: [[sum("wal" in d[3] for d in docs)]],
        "materialize": lambda: [[d[0], d[3]] for d in docs if d[2] == "rag"],
        "join_inner": lambda: [[o[0], emails[o[1]], o[2]] for o in orders],
        "join_filter": lambda: [[o[0], emails[o[1]], o[2]] for o in orders if o[2] > 100],
    }[case]()


def validate(got, want, engine, case):
    # Row order is not a query contract; duplicates, columns and values are.
    if sorted(map(tuple, got)) != sorted(map(tuple, want)):
        raise AssertionError(f"{engine}/{case}: result mismatch ({len(got)} vs {len(want)} rows)")


def measure(fn, samples, target_ms):
    fn()  # warm query/cache/connection
    start = time.perf_counter_ns()
    fn()
    elapsed = max(1, time.perf_counter_ns() - start)
    iterations = max(1, min(10000, int(target_ms * 1e6 / elapsed)))
    timings = []
    for _ in range(samples):
        start = time.perf_counter_ns()
        for _ in range(iterations):
            fn()
        timings.append((time.perf_counter_ns() - start) / iterations)
    return timings, iterations


class Lin:
    def __init__(self, data, args, prefix):
        binary = Path(args.lin_binary)
        if not binary.is_file():
            raise RuntimeError("build worker: cargo build --release --example peer_bench")
        self.proc = subprocess.Popen([str(binary.resolve())], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, text=True)
        try:
            self.version = self.ask({"rows": len(data[0])})["version"]
        except BaseException:
            self.close()
            raise

    def ask(self, request):
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("Lin worker exited")
        return json.loads(line)

    def measure(self, case, samples, target_ms):
        source, fields = {
            "point_get": ('docs | id == "d-20" | { id, title }', ["id", "title"]),
            "filter_eq": ('docs | wing == "rag" | count', ["$count"]),
            "text_substr": ('docs | title ~ "wal" | count', ["$count"]),
            "materialize": ('docs | wing == "rag" | { id, title } | take all', ["id", "title"]),
            "join_inner": ('orders | join users on user_id | { id, users.email, total } | take all', ["id", "users.email", "total"]),
            "join_filter": ('orders | total > 100 | join users on user_id | { id, users.email, total } | take all', ["id", "users.email", "total"]),
        }[case]
        request = {"source": source, "fields": fields, "iterations": 1, "samples": 1}
        probe = self.ask(request)
        iterations = max(1, min(10000, int(target_ms * 1e6 / max(1, probe["samples_ns"][0]))))
        result = self.ask({**request, "iterations": iterations, "samples": samples})
        return result["samples_ns"], iterations, result["result"]

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        self.proc.stdout.close()


class Sql:
    def __init__(self, data, args, prefix, engine):
        self.engine = engine
        self.tables = [prefix + "_" + name for name in ["docs", "users", "orders"]]
        self.created = []
        if engine == "sqlite":
            import sqlite3
            self.conn = sqlite3.connect(":memory:")
            self.version = sqlite3.sqlite_version
        elif engine == "duckdb":
            import duckdb
            self.conn = duckdb.connect(":memory:")
            self.version = duckdb.__version__
        elif engine == "postgres":
            import psycopg
            self.conn = psycopg.connect(os.environ.get("LIN_BENCH_PG_URL", "postgresql://lin:lin@127.0.0.1:55432/lin"), connect_timeout=5, autocommit=True)
            self.version = str(self.conn.info.server_version)
        elif engine == "mysql":
            import pymysql
            url = urllib.parse.urlsplit(os.environ.get("LIN_BENCH_MYSQL_URL", "mysql://lin:lin@127.0.0.1:53306/lin"))
            self.conn = pymysql.connect(host=url.hostname, port=url.port or 3306,
                user=urllib.parse.unquote(url.username or ""), password=urllib.parse.unquote(url.password or ""),
                database=url.path.lstrip("/"), connect_timeout=5, autocommit=True)
            self.version = self.conn.get_server_info()
        else:
            host = os.environ.get("LIN_BENCH_MSSQL_HOST")
            if not host:
                raise RuntimeError("set LIN_BENCH_MSSQL_HOST/USER/PASSWORD/DATABASE (dedicated test server)")
            import pytds
            self.conn = pytds.connect(server=host, port=int(os.environ.get("LIN_BENCH_MSSQL_PORT", "1433")),
                user=os.environ["LIN_BENCH_MSSQL_USER"], password=os.environ["LIN_BENCH_MSSQL_PASSWORD"],
                database=os.environ["LIN_BENCH_MSSQL_DATABASE"], login_timeout=5, autocommit=True)
            self.version = "pending server version query"
        self.cursor = self.conn.cursor()
        try:
            if engine == "mssql":
                self.cursor.execute("SELECT CAST(SERVERPROPERTY('ProductVersion') AS VARCHAR(128))")
                self.version = self.cursor.fetchone()[0]
            schema = ["id VARCHAR(64) PRIMARY KEY, uri VARCHAR(128), wing VARCHAR(16), title VARCHAR(128), body VARCHAR(128)",
                      "id VARCHAR(64) PRIMARY KEY, email VARCHAR(128)",
                      "id VARCHAR(64) PRIMARY KEY, user_id VARCHAR(64), total BIGINT"]
            marker = "?" if engine in ["sqlite", "duckdb"] else "%s"
            for table, columns, rows in zip(self.tables, schema, data):
                self.cursor.execute(f"CREATE TABLE {table} ({columns})")
                self.created.append(table)
                self.cursor.executemany(f"INSERT INTO {table} VALUES ({','.join([marker] * len(rows[0]))})", rows)
            self.cursor.execute(f"CREATE INDEX {prefix}_wing ON {self.tables[0]} (wing)")
            self.conn.commit()
            d, u, o = self.tables
            self.queries = {
                "point_get": f"SELECT id,title FROM {d} WHERE id = 'd-20'",
                "filter_eq": f"SELECT COUNT(*) FROM {d} WHERE wing = 'rag'",
                "text_substr": f"SELECT COUNT(*) FROM {d} WHERE title LIKE '%wal%'",
                "materialize": f"SELECT id,title FROM {d} WHERE wing = 'rag'",
                "join_inner": f"SELECT o.id,u.email,o.total FROM {o} o INNER JOIN {u} u ON o.user_id=u.id",
                "join_filter": f"SELECT o.id,u.email,o.total FROM {o} o INNER JOIN {u} u ON o.user_id=u.id WHERE o.total>100",
            }
            if engine == "duckdb":
                for case, query in self.queries.items():
                    self.cursor.execute(f"PREPARE {case} AS {query}")
                self.queries = {case: f"EXECUTE {case}" for case in self.queries}
        except BaseException:
            self.close()
            raise

    def query(self, case):
        if self.engine == "postgres":
            self.cursor.execute(self.queries[case], prepare=True)
        else:
            self.cursor.execute(self.queries[case])
        return [list(row) for row in self.cursor.fetchall()]

    def close(self):
        try:
            for table in reversed(self.created):
                self.cursor.execute(f"DROP TABLE {table}")
            self.conn.commit()
        finally:
            self.cursor.close()
            self.conn.close()


class Pandas:
    def __init__(self, data, args, prefix):
        import pandas as pd
        self.version = pd.__version__
        self.docs = pd.DataFrame(data[0], columns=["id", "uri", "wing", "title", "body"])
        self.by_id = self.docs.set_index("id")
        self.users = pd.DataFrame(data[1], columns=["user_id", "email"])
        self.orders = pd.DataFrame(data[2], columns=["id", "user_id", "total"])

    def query(self, case):
        if case == "point_get":
            return [["d-20", self.by_id.at["d-20", "title"]]]
        if case == "filter_eq":
            return [[int((self.docs.wing == "rag").sum())]]
        if case == "text_substr":
            return [[int(self.docs.title.str.contains("wal", regex=False).sum())]]
        if case == "materialize":
            frame = self.docs.loc[self.docs.wing == "rag", ["id", "title"]]
        else:
            orders = self.orders if case == "join_inner" else self.orders[self.orders.total > 100]
            frame = orders.merge(self.users, on="user_id", how="inner")[["id", "email", "total"]]
        return [list(row) for row in frame.itertuples(index=False, name=None)]

    def close(self):
        pass


class Mongo:
    def __init__(self, data, args, prefix):
        from pymongo import MongoClient
        self.client = MongoClient(os.environ.get("LIN_BENCH_MONGO_URL", "mongodb://127.0.0.1:27027"), serverSelectionTimeoutMS=5000)
        self.db = self.client[prefix]
        try:
            self.version = self.client.server_info()["version"]
            for name, rows, columns in zip(["docs", "users", "orders"], data,
                    [["id", "uri", "wing", "title", "body"], ["id", "email"], ["id", "user_id", "total"]]):
                self.db[name].insert_many([dict(zip(columns, row)) for row in rows])
                self.db[name].create_index("id", unique=True)
            self.db.docs.create_index("wing")
        except BaseException:
            self.close()
            raise

    def query(self, case):
        if case == "point_get":
            row = self.db.docs.find_one({"id": "d-20"}, {"_id": 0, "id": 1, "title": 1})
            return [[row["id"], row["title"]]]
        if case == "filter_eq":
            return [[self.db.docs.count_documents({"wing": "rag"})]]
        if case == "text_substr":
            return [[self.db.docs.count_documents({"title": {"$regex": "wal"}})]]
        if case == "materialize":
            return [[r["id"], r["title"]] for r in self.db.docs.find({"wing": "rag"}, {"_id": 0, "id": 1, "title": 1})]
        pipeline = [] if case == "join_inner" else [{"$match": {"total": {"$gt": 100}}}]
        pipeline += [{"$lookup": {"from": "users", "localField": "user_id", "foreignField": "id", "as": "user"}},
                     {"$unwind": "$user"}, {"$project": {"_id": 0, "id": 1, "email": "$user.email", "total": 1}}]
        return [[r["id"], r["email"], r["total"]] for r in self.db.orders.aggregate(pipeline)]

    def close(self):
        try:
            self.client.drop_database(self.db.name)
        finally:
            self.client.close()


class NativeInsert(Lin):
    """Fresh fixtures; real native write plus exact readback on every sample."""
    def __init__(self, data, args, prefix, engine):
        self.engine = engine
        self.proc = None
        self.client = None
        self.db = None
        self.owns_database = False
        self.timestamp = args.native_timestamp_ms
        self.want = [[d[0], d[1], d[2], d[3], self.timestamp, d[4]] for d in data[0]]
        self.contract = {"schema": "id/uri/wing/title/ts/body; unique id and uri; wing+ts index",
                         "fixture": "fresh per sample; schema/index/prepare/validation/drop excluded",
                         "durability": "native memory API comparison; no disk durability equivalence"}
        try:
            if engine == "lin":
                binary = Path(args.lin_binary).resolve()
                self.proc = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
                self.version = self.ask({"worker_version": True})["version"]
                self.contract.update({"api": "Rust prepared run; default embedding and FTS", "storage": "Db::empty"})
            elif engine == "duckdb":
                import duckdb
                import pandas as pd
                self.version = duckdb.__version__
                self.frame = pd.DataFrame(self.want, columns=["id", "uri", "wing", "title", "ts", "body"])
                self.contract.update({"api": "DuckDB prepared INSERT SELECT from registered pandas DataFrame plus commit", "storage": ":memory:", "atomicity": "one transaction per sample"})
            elif engine == "sqlite":
                import sqlite3
                self.version = sqlite3.sqlite_version
                self.contract.update({"api": "Python sqlite3 executemany plus transaction commit", "storage": ":memory:", "atomicity": "one transaction per sample"})
            elif engine == "mongo":
                from pymongo import MongoClient
                from pymongo.write_concern import WriteConcern
                self.client = MongoClient(os.environ.get("LIN_BENCH_MONGO_URL", "mongodb://127.0.0.1:27027"), serverSelectionTimeoutMS=5000)
                self.version = self.client.server_info()["version"]
                if prefix in self.client.list_database_names():
                    raise RuntimeError("refuse existing Mongo fixture database")
                self.db = self.client[prefix]
                self.db.create_collection("__linbench_owner")
                self.owns_database = True
                self.concern = WriteConcern(w=1, j=False)
                self.contract.update({"api": "PyMongo ordered insert_many; id maps to native _id", "write_concern": self.concern.document})
            else:
                raise RuntimeError("native insert is not implemented for " + engine)
        except BaseException:
            self.close()
            raise

    def measure_insert(self, samples):
        if self.engine == "lin":
            result = self.ask({"insert_rows": len(self.want), "samples": samples, "timestamp_ms": self.timestamp})
            validate(result["result"], self.want, self.engine, "insert_native")
            return result["samples_ns"], 1, result["result"]
        if self.engine == "duckdb":
            import duckdb
            timings = []
            got = None
            for _ in range(samples):
                conn = duckdb.connect(":memory:")
                try:
                    conn.execute("CREATE TABLE docs (id VARCHAR PRIMARY KEY, uri VARCHAR UNIQUE, wing VARCHAR, title VARCHAR, ts BIGINT, body VARCHAR)")
                    conn.execute("CREATE INDEX docs_wing_ts ON docs(wing, ts)")
                    conn.register("input_docs", self.frame)
                    conn.execute("PREPARE insert_docs AS INSERT INTO docs SELECT id, uri, wing, title, ts, body FROM input_docs")
                    start = time.perf_counter_ns()
                    conn.execute("BEGIN TRANSACTION")
                    conn.execute("EXECUTE insert_docs")
                    conn.commit()
                    timings.append(time.perf_counter_ns() - start)
                    got = [list(row) for row in conn.execute("SELECT id, uri, wing, title, ts, body FROM docs").fetchall()]
                    validate(got, self.want, self.engine, "insert_native")
                finally:
                    conn.close()
            return timings, 1, got
        if self.engine == "sqlite":
            import sqlite3
            timings = []
            got = None
            for _ in range(samples):
                conn = sqlite3.connect(":memory:")
                try:
                    conn.execute("CREATE TABLE docs (id TEXT PRIMARY KEY, uri TEXT UNIQUE, wing TEXT, title TEXT, ts INTEGER, body TEXT)")
                    conn.execute("CREATE INDEX docs_wing_ts ON docs(wing, ts)")
                    cursor = conn.cursor()
                    start = time.perf_counter_ns()
                    cursor.executemany("INSERT INTO docs VALUES (?, ?, ?, ?, ?, ?)", self.want)
                    conn.commit()
                    timings.append(time.perf_counter_ns() - start)
                    got = [list(row) for row in conn.execute("SELECT id, uri, wing, title, ts, body FROM docs")]
                    validate(got, self.want, self.engine, "insert_native")
                finally:
                    conn.close()
            return timings, 1, got
        timings = []
        docs = [dict(zip(["_id", "uri", "wing", "title", "ts", "body"], row)) for row in self.want]
        got = None
        for sample in range(samples):
            name = "docs_" + str(sample)
            created = False
            try:
                collection = self.db.create_collection(name, write_concern=self.concern)
                created = True
                collection.create_index("uri", unique=True)
                collection.create_index([("wing", 1), ("ts", 1)])
                start = time.perf_counter_ns()
                result = collection.insert_many(docs, ordered=True)
                timings.append(time.perf_counter_ns() - start)
                if not result.acknowledged or len(result.inserted_ids) != len(self.want):
                    raise RuntimeError("Mongo insert acknowledgement/count mismatch")
                got = [[d[k] for k in ["_id", "uri", "wing", "title", "ts", "body"]] for d in collection.find({})]
                validate(got, self.want, self.engine, "insert_native")
            finally:
                if created:
                    self.db.drop_collection(name)
        return timings, 1, got

    def close(self):
        if self.proc is not None:
            super().close()
            self.proc = None
        if self.client is not None:
            try:
                if self.db is not None and self.owns_database:
                    self.client.drop_database(self.db.name)
            finally:
                self.client.close()
                self.client = None


class Kusto:
    def __init__(self, data, args, prefix):
        self.endpoint = os.environ.get("LIN_BENCH_KUSTO_URL")
        self.database = os.environ.get("LIN_BENCH_KUSTO_DATABASE")
        if not self.endpoint or not self.database:
            raise RuntimeError("set LIN_BENCH_KUSTO_URL/DATABASE and optionally TOKEN (dedicated test database)")
        url = urllib.parse.urlsplit(self.endpoint)
        if url.scheme not in ["http", "https"] or not url.hostname:
            raise RuntimeError("Kusto endpoint must be an HTTP(S) URL")
        if url.scheme == "http" and os.environ.get("LIN_BENCH_KUSTO_TOKEN") and url.hostname not in ["localhost", "127.0.0.1", "::1"]:
            raise RuntimeError("remote authenticated Kusto requires HTTPS")
        connection = http.client.HTTPSConnection if url.scheme == "https" else http.client.HTTPConnection
        self.connection = connection(url.hostname, port=url.port, timeout=60)
        self.base_path = url.path.rstrip("/")
        self.created = []
        self.tables = [prefix + "_" + name for name in ["docs", "users", "orders"]]
        try:
            self.version = self.execute(".show version", management=True)
            for table, columns, rows in zip(self.tables,
                    ["id:string,uri:string,wing:string,title:string,body:string", "id:string,email:string", "id:string,user_id:string,total:long"], data):
                self.execute(f".create table {table} ({columns})", management=True)
                self.created.append(table)
                for start in range(0, len(rows), 1000):
                    buf = io.StringIO()
                    csv.writer(buf, lineterminator="\n").writerows(rows[start:start+1000])
                    self.execute(f".ingest inline into table {table} <|\n{buf.getvalue()}", management=True)
                deadline = time.monotonic() + 60
                while self.execute(f"{table} | count") != [[len(rows)]]:
                    if time.monotonic() >= deadline:
                        raise RuntimeError("Kusto ingest visibility timeout")
                    time.sleep(0.25)
            d, u, o = self.tables
            self.queries = {
                "point_get": f'{d} | where id == "d-20" | project id,title',
                "filter_eq": f'{d} | where wing == "rag" | count',
                "text_substr": f'{d} | where title contains_cs "wal" | count',
                "materialize": f'{d} | where wing == "rag" | project id,title',
                "join_inner": f'{o} | join kind=inner ({u}) on $left.user_id == $right.id | project id,email,total',
                "join_filter": f'{o} | where total>100 | join kind=inner ({u}) on $left.user_id == $right.id | project id,email,total',
            }
        except BaseException:
            self.close()
            raise

    def execute(self, query, management=False):
        token = os.environ.get("LIN_BENCH_KUSTO_TOKEN")
        headers = {"Content-Type": "application/json; charset=utf-8", "Accept": "application/json"}
        if token:
            headers["Authorization"] = "Bearer " + token
        body = json.dumps({"db": self.database, "csl": query, "properties": json.dumps({"Options": {"query_results_cache_max_age": "00:00:00", "notruncation": True}})}).encode()
        self.connection.request("POST", self.base_path + ("/v1/rest/mgmt" if management else "/v1/rest/query"), body, headers)
        response = self.connection.getresponse()
        raw = response.read()
        if response.status != 200:
            raise RuntimeError(f"Kusto HTTP status {response.status}")
        result = json.loads(raw)
        if "error" in result:
            raise RuntimeError("Kusto command/query failed")
        tables = result.get("Tables", [])
        for table in tables:
            if table.get("TableName") == "QueryStatus":
                columns = [column["ColumnName"] for column in table.get("Columns", [])]
                severity = columns.index("Severity")
                code = columns.index("StatusCode")
                if any(row[severity] <= 2 or row[code] != 0 for row in table.get("Rows", [])):
                    raise RuntimeError("Kusto query status reported an error")
        if not tables:
            raise RuntimeError("Kusto returned no result tables")
        primary = next((table for table in tables if table.get("TableName") == "PrimaryResult"), tables[0] if tables else {})
        return primary.get("Rows", [])

    def query(self, case):
        return self.execute(self.queries[case])

    def close(self):
        try:
            for table in reversed(self.created):
                self.execute(f".drop table {table} ifexists", management=True)
        finally:
            self.connection.close()


def report(data):
    native = data["cases"] == ["insert_native"]
    title = "# Validated native insert API benchmark" if native else "# Validated read API benchmark"
    lines = [title, "", f"Rows: {data['rows']}; process repetitions: {data['repeats']}; host: {data['host']}", "",
             ("Fresh fixture per sample; schema/index/preparation, exact readback, drop and Lin JSON IPC excluded. Writes store the same six common fields." if native else "Setup, validation and Lin JSON IPC excluded. Queries return the same materialized columns/values."),
             "Lin: Rust prepared API; Python peers: DBAPI/DataFrame/driver API. Server timings include round-trip and transfer.",
             "Medians are batch averages, not individual latency percentiles. Each process is aggregated first.", "",
             "| Case | Engine | Median µs/op | Peer / Lin | Status |", "|---|---|---:|---:|---|"]
    for case in data["cases"]:
        lin = data["medians_ns"].get(f"{case}/lin")
        for engine in data["engines"]:
            value = data["medians_ns"].get(f"{case}/{engine}")
            if value is not None:
                ratio = value / lin if lin else None
                lines.append(f"| {case} | {engine} | {value / 1000:.3f} | {ratio:.2f}× | validated |" if ratio else f"| {case} | {engine} | {value / 1000:.3f} | — | validated |")
            else:
                lines.append(f"| {case} | {engine} | — | — | unavailable/error |")
    if data.get("native_contracts"):
        lines += ["", "Native contracts: " + json.dumps(data["native_contracts"], sort_keys=True)]
    lines += ["", "Versions: " + json.dumps(data["versions"], sort_keys=True), "", "Missing/failed peers are never counted as wins."]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engines", nargs="+", choices=ENGINES, default=ENGINES)
    parser.add_argument("--cases", nargs="+", choices=CASES + ["insert_native"], default=CASES)
    parser.add_argument("--rows", type=int, default=10000)
    parser.add_argument("--samples", type=int, default=8)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--sample-ms", type=float, default=5)
    parser.add_argument("--lin-binary", default=str(ROOT / "target/release/examples/peer_bench"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--require-wins", action="store_true", help="fail if any measured peer beats Lin")
    args = parser.parse_args()
    if args.rows <= 20 or min(args.samples, args.repeats, args.sample_ms) <= 0:
        parser.error("rows must exceed 20; samples/repeats/sample-ms must be positive")
    if "insert_native" in args.cases and args.cases != ["insert_native"]:
        parser.error("run insert_native separately from read cases")
    args.native_timestamp_ms = time.time_ns() // 1_000_000
    args.output.mkdir(parents=True, exist_ok=False)
    run = {"rows": args.rows, "repeats": args.repeats, "samples": args.samples,
           "host": platform.platform(), "engines": args.engines, "cases": args.cases,
           "observations": [], "errors": [], "versions": {}, "medians_ns": {},
           "python": sys.version, "clients": client_versions()}
    if args.repeats > 1:
        for repetition in range(args.repeats):
            order = args.engines[repetition % len(args.engines):] + args.engines[:repetition % len(args.engines)]
            child_out = args.output / f"process-{repetition+1}"
            command = [sys.executable, str(Path(__file__).resolve()), "--engines", *order,
                       "--cases", *args.cases, "--rows", str(args.rows), "--samples", str(args.samples),
                       "--sample-ms", str(args.sample_ms), "--repeats", "1",
                       "--lin-binary", args.lin_binary, "--output", str(child_out)]
            print(f"Independent process {repetition+1}/{args.repeats}", flush=True)
            completed = subprocess.run(command, check=False)
            child_json = child_out / "run.json"
            if not child_json.is_file():
                run["errors"].append({"repetition": repetition, "reason": "worker failed before report"})
                continue
            child = json.loads(child_json.read_text())
            for observation in child["observations"]:
                run["observations"].append({**observation, "repetition": repetition})
            for error in child["errors"]:
                run["errors"].append({**error, "repetition": repetition})
            if completed.returncode not in [0, 2]:
                run["errors"].append({"repetition": repetition, "reason": "worker exit " + str(completed.returncode)})
            run["versions"].update(child["versions"])
            run.setdefault("native_contracts", {}).update(child.get("native_contracts", {}))
            (args.output / "run.json").write_text(json.dumps(run, indent=2) + "\n")
        return finish(args, run)
    data = dataset(args.rows)
    for repetition in range(args.repeats):
        # Rotate engine order across independent worker/connection repetitions.
        order = args.engines[repetition % len(args.engines):] + args.engines[:repetition % len(args.engines)]
        for engine in order:
            peer = None
            try:
                prefix = "linbench_" + uuid.uuid4().hex[:16]
                if args.cases == ["insert_native"]:
                    peer = NativeInsert(data, args, prefix, engine)
                    run.setdefault("native_contracts", {})[engine] = peer.contract
                elif engine == "lin":
                    peer = Lin(data, args, prefix)
                elif engine in ["sqlite", "duckdb", "postgres", "mysql", "mssql"]:
                    peer = Sql(data, args, prefix, engine)
                else:
                    peer = {"pandas": Pandas, "mongo": Mongo, "kusto": Kusto}[engine](data, args, prefix)
                run["versions"][engine] = peer.version
                for case in args.cases:
                    want = peer.want if case == "insert_native" else expected(data, case)
                    if case == "insert_native":
                        values, iterations, got = peer.measure_insert(args.samples)
                    elif engine == "lin":
                        values, iterations, got = peer.measure(case, args.samples, args.sample_ms)
                    else:
                        validate(peer.query(case), want, engine, case)
                        values, iterations = measure(lambda: peer.query(case), args.samples, args.sample_ms)
                        got = peer.query(case)
                    validate(got, want, engine, case)
                    run["observations"].append({"case": f"{case}/{engine}", "repetition": repetition,
                        "iterations": iterations, "samples_ns": values, "validated_rows": len(want)})
                    print(f"{repetition+1}/{args.repeats} {case}/{engine}: {statistics.median(values)/1000:.3f} µs", flush=True)
            except Exception as error:
                # Driver exception strings may contain connection credentials: keep only type.
                reason = str(error) if type(error) is RuntimeError else type(error).__name__
                run["errors"].append({"engine": engine, "repetition": repetition, "reason": reason})
                print(f"{engine}: unavailable/failed ({type(error).__name__})", file=sys.stderr, flush=True)
            finally:
                if peer is not None:
                    try:
                        peer.close()
                    except Exception as error:
                        run["errors"].append({"engine": engine, "repetition": repetition, "reason": "cleanup: " + type(error).__name__})
            (args.output / "run.json").write_text(json.dumps(run, indent=2) + "\n")
    return finish(args, run)


def finish(args, run):
    for case in args.cases:
        for engine in args.engines:
            rows = [o for o in run["observations"] if o["case"] == f"{case}/{engine}"]
            if len(rows) == args.repeats:
                run["medians_ns"][f"{case}/{engine}"] = statistics.median(statistics.median(o["samples_ns"]) for o in rows)
    complete = len(run["medians_ns"]) == len(args.cases) * len(args.engines) and not run["errors"]
    run["status"] = "complete" if complete else "incomplete"
    (args.output / "run.json").write_text(json.dumps(run, indent=2) + "\n")
    (args.output / "report.md").write_text(report(run))
    if not complete:
        return 2
    if args.require_wins:
        for case in args.cases:
            lin = run["medians_ns"].get(f"{case}/lin")
            if lin is None or any(v < lin for k, v in run["medians_ns"].items() if k.startswith(case + "/") and not k.endswith("/lin")):
                return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
