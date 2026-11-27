"""Process orchestration only; all measured operations and clocks are in Rust."""
import json
from pathlib import Path
import subprocess
import tempfile

samples = []
for peer,sizes in (("sqlite",(1000,10000)),("duckdb",(10000,100000))):
    for n in sizes:
        for pair in range(12):
            sample=dict(peer=peer,n=n)
            for engine in (("lin",peer) if pair%2==0 else (peer,"lin")):
                with tempfile.TemporaryDirectory() as directory:
                    count=1 if peer=="sqlite" else 5
                    if engine=="lin":
                        request=dict(insert_rows=n,samples=count,timestamp_ms=123456)
                        if peer=="sqlite":
                            request["durable_dir"]=directory
                        result=json.loads(subprocess.check_output(["target/release/lin-peer-worker"],input=json.dumps(request),text=True))
                        assert len(result["result"])==n
                        if peer=="sqlite":
                            assert result["reopen_ns"][0]>0
                        sample[engine]=result["samples_ns"]
                    else:
                        sample[engine]=json.loads(subprocess.check_output(["target/release/embedded-native",peer,str(n),str(count),directory]))
                    assert len(sample[engine])==count and min(sample[engine])>0
            samples.append(sample)
            Path("embedded-results.json").write_text(json.dumps(samples))
