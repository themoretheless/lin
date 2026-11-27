import json
import subprocess
import tempfile
from time import perf_counter_ns
from pytds import tds_base, tds_types


def measure(connection, worker, output):
    names = ["id", "uri", "wing", "title", "ts", "body", "layer"]
    widths = [64, 128, 16, 128, None, 128, 16]
    columns = [tds_base.Column(name=name,
               type=tds_types.NVarCharType(width) if width else tds_types.BigIntType(),
               flags=0 if name == "id" else tds_base.Column.fNullable)
               for name, width in zip(names, widths)]
    samples = []
    for n in (1000, 10000, 100000):
        rows = [(f"d-{i}", f"bench://{i}", "rag" if i%2==0 else "sys",
                 f"doc {i} " + ("wal note" if i%10==0 else "plain"), 123456, f"body {i}", "wiki") for i in range(n)]
        for rep in range(12):
            sample = dict(n=n)
            for engine in (("lin", "mssql") if rep%2==0 else ("mssql","lin")):
                if engine == "lin":
                    with tempfile.TemporaryDirectory() as d:
                        sample[engine] = int(subprocess.check_output([worker,d,str(n)]))
                else:
                    with connection.cursor() as c:
                        c.execute("CREATE TABLE dbo.docs(id nvarchar(64) PRIMARY KEY,uri nvarchar(128) UNIQUE,wing nvarchar(16),title nvarchar(128),ts bigint,body nvarchar(128),layer nvarchar(16))")
                        c.execute("CREATE INDEX docs_wing_ts ON dbo.docs(wing,ts)")
                        start = perf_counter_ns()
                        for i in range(0,n,10000):
                            c.execute("BEGIN TRANSACTION")
                            c.copy_to(table_or_view="docs",schema="dbo",columns=columns,data=rows[i:i+10000],check_constraints=True,fire_triggers=True,keep_nulls=True)
                            c.execute("COMMIT TRANSACTION")
                        sample[engine] = perf_counter_ns()-start
                        c.execute("SELECT id,uri,wing,title,ts,body,layer FROM dbo.docs")
                        assert sorted(c.fetchall()) == sorted(rows)
                        c.execute("DROP TABLE dbo.docs")
            samples.append(sample)
            (output/"tcp-pairs.json").write_text(json.dumps(samples))
