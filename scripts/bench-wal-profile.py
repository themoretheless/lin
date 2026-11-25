from pathlib import Path
import json
import statistics
import subprocess
import tempfile

import sys

binary = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
records = []
for size in (1000, 10000):
    for i in range(12):
        with tempfile.TemporaryDirectory(dir=out) as directory:
            proc = subprocess.run([binary, str(size), directory], capture_output=True, text=True)
        record = dict(rows=size, round=i, stdout=proc.stdout, stderr=proc.stderr, status=proc.returncode)
        records.append(record)
        (out/'wal-profile-raw.json').write_text(json.dumps(records, indent=2)+'\n')
        proc.check_returncode()
        encodes = [line.split() for line in proc.stderr.splitlines() if line.startswith('WAL_ENCODE ')]
        picked = [x for x in encodes if int(x[1])==size and x[3]=='true']
        assert len(picked)==1, record
        frames = [line.split() for line in proc.stderr.splitlines() if line.startswith('WAL_FRAME ')]
        matched = [x for x in frames if x[1]==picked[0][4] and x[4]=='Full']
        assert len(matched)==1, record
        record['ns'] = dict(total=float(proc.stdout.strip()), encode=int(picked[0][2]),
                            write=int(matched[0][2]), sync=int(matched[0][3]))
        record['wal_payload_bytes'] = int(matched[0][1])
        (out/'wal-profile-raw.json').write_text(json.dumps(records, indent=2)+'\n')
for size in (1000, 10000):
    data = [x['ns'] for x in records if x['rows']==size]
    print(size, {k:statistics.median(x[k] for x in data)/1e6 for k in data[0]})
