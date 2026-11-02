from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2];m=json.loads((p/'input.json').read_text())
for name,h in m['files_sha256'].items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==h,name
for name,h in json.loads((p/'runtime-inputs.json').read_text())['runtime_sha256'].items():assert hashlib.sha256((root/name).read_bytes()).hexdigest()==h,name
print('PASS all frozen current-source, worker, driver, workflow and prerequisite inputs')
