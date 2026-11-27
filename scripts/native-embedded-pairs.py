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
                    mode = ("lin-wal" if peer == "sqlite" else "lin-memory") if engine == "lin" else peer
                    sample[engine] = json.loads(subprocess.check_output(
                        ["target/release/embedded-native", mode, str(n), str(count), directory]))
                    assert len(sample[engine])==count and min(sample[engine])>0
            samples.append(sample)
            Path("embedded-results.json").write_text(json.dumps(samples))
