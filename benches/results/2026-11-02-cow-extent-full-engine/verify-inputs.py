from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2]
for name,h in json.loads((p/'frozen.json').read_text()).items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==h,name
print('PASS all frozen source, test, harness and workflow inputs')
