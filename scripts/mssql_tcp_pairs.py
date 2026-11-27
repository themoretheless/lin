"""Orchestrate fresh native client processes. Timers and validation live in Rust."""
import json
import subprocess
import tempfile

def measure(connection, worker, output, env):
    samples = []
    for n in (1000, 10000, 100000):
        for i in range(12):
            sample = dict(n=n)
            for engine in (("lin", "mssql") if i%2==0 else ("mssql","lin")):
                with tempfile.TemporaryDirectory() as directory:
                    cmd = ([worker,directory,str(n)] if engine=="lin" else
                           [worker.with_name("sql-tcp-full"),engine,str(n)])
                    sample[engine] = int(subprocess.check_output(cmd, env=env))
            samples.append(sample)
            (output/"tcp-pairs.json").write_text(json.dumps(samples))
