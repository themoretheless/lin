import csv
import io
import json
import subprocess
import tempfile
import time
from pathlib import Path
import psycopg

def postgres(db, rows):
    db.execute("DROP TABLE IF EXISTS docs")
    db.execute("CREATE TABLE docs(id text PRIMARY KEY, uri text UNIQUE, wing text, title text, ts bigint, body text, layer text)")
    db.execute("CREATE INDEX docs_wing_ts ON docs(wing, ts)")
    start = time.perf_counter_ns()
    data = io.StringIO(newline="")
    csv.writer(data, quoting=csv.QUOTE_ALL).writerows(rows)
    payload = data.getvalue().encode()
    with db.transaction(), db.cursor() as cursor:
        with cursor.copy("COPY docs FROM STDIN WITH (FORMAT CSV)") as copy:
            copy.write(payload)
    elapsed = time.perf_counter_ns() - start
    assert sorted(db.execute("SELECT * FROM docs").fetchall()) == sorted(rows)
    return elapsed

with psycopg.connect("postgresql://lin:lin@127.0.0.1:55432/lin", autocommit=True) as db:
    for setting in ("fsync", "synchronous_commit", "full_page_writes"):
        assert db.execute("SHOW "+setting).fetchone()[0] == "on"
    report = dict(server=db.info.server_version, samples=[])
    for n in (1000, 10000, 100000):
        rows = [(f"d-{i}", f"bench://{i}", "rag" if i%2==0 else "sys",
                 f"doc {i} " + ("wal note" if i%10==0 else "plain"), 123456, f"body {i}", "wiki") for i in range(n)]
        for i in range(12):
            sample = dict(n=n)
            for engine in (("lin","pg") if i%2==0 else ("pg","lin")):
                with tempfile.TemporaryDirectory() as directory:
                    sample[engine] = (int(subprocess.check_output(["target/release/tcp-full",directory,str(n)]))
                                      if engine=="lin" else postgres(db,rows))
            report["samples"].append(sample)
            Path("tcp-results.json").write_text(json.dumps(report))
