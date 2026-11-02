from pathlib import Path
import ast,json,hashlib,subprocess
from source_manifest import manifest
p=Path(__file__).resolve().parent;root=p.parents[2];data=manifest()
for name in ['tools/peer-worker/src/main.rs','tools/peer-worker/Cargo.toml','examples/peer_bench.rs','lin-derive/src/lib.rs','Cargo.toml','Cargo.lock','scripts/bench-peers.py']:assert name in data,name
for name in ['build.py','run.py']:
 tree=ast.parse((p/name).read_text());calls=[n for n in ast.walk(tree)if isinstance(n,ast.Call)and isinstance(n.func,ast.Name)and n.func.id=='manifest'];assert calls,name
proof=json.loads((p/'timed-helper-proof.json').read_text())
for name,h in proof['unchanged_helpers'].items():
 actual=(p/name).read_bytes();assert hashlib.sha256(actual).hexdigest()==h;assert actual==subprocess.check_output(['git','show',proof['previous_ref']+':benches/results/2026-11-02-corrected-cow-peer-integrity-v2/'+name],cwd=root)
runtime=json.loads((p/'runtime-inputs.json').read_text())
for name,h in runtime['runtime_sha256'].items():
 assert hashlib.sha256((root/name).read_bytes()).hexdigest()==h
 assert hashlib.sha256(subprocess.check_output(['git','show',runtime['runtime_reference']+':'+name],cwd=root)).hexdigest()==h
assert runtime['features']==['default','experimental-extent-wal']
tree=ast.parse((p/'build.py').read_text());command=next(ast.literal_eval(n.value)for n in tree.body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='command'for t in n.targets));assert command[command.index('--features')+1]=='experimental-extent-wal'
print('PASS shared complete worker/build manifests and byte-identical read/native timed helpers')
