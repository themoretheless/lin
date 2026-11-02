from pathlib import Path
import hashlib,importlib.util,json,platform
p=Path(__file__).resolve().parent;root=p.parents[2]
prior=root/'benches/results/2026-11-02-mongo-ubuntu22-readiness-linux'
s=json.loads((prior/'status.json').read_text());assert all(s[k]for k in ['ready','ping_verified','crud_verified','cleanup_verified'])and s['benchmark_timings']==0
meta=json.loads((prior/'run-metadata.json').read_text());assert meta['databaseId']==41373432189 and meta['headSha']=='8bcf665a7d0dec4d3c695840ca148988481de0c7'and meta['conclusion']=='success'
input=json.loads((p/'input.json').read_text());assert input['images']['mongo']==s['image']
script=root/'benches/results/2026-11-02-mongo-ubuntu22-readiness/probe.py'
spec=importlib.util.spec_from_file_location('guarded_mongo_probe',script);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
assert platform.system()=='Linux'and platform.machine()=='x86_64'
affected=module.kernel_affected(platform.release())
record=dict(platform=platform.platform(),kernel=platform.release(),architecture=platform.machine(),kernel_guard_passed=not affected,mongo_readiness_prerequisite_run=41373432189,mongo_image=input['images']['mongo'],benchmark_timings=0)
(p/'host-check.json').write_text(json.dumps(record,indent=2)+'\n')
assert not affected,'Known affected kernel: do not start MongoDB'
print(json.dumps(record))
