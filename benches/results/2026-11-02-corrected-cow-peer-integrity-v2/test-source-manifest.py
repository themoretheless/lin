"""Evaluate only source-manifest expressions, without starting servers or builds."""
from pathlib import Path
import ast,hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[2]
expected={str(f)for f in Path('src').rglob('*.rs')}|{'Cargo.toml','Cargo.lock','examples/peer_bench.rs','scripts/bench-peers.py','scripts/benchmark_source_integrity.py','scripts/test_benchmark_source_integrity.py'}
for name,target in [('build.py','files'),('run.py','source')]:
 tree=ast.parse((p/name).read_text());nodes=[]
 for node in tree.body:
  if isinstance(node,ast.Assign)and any(isinstance(t,ast.Name)and t.id==target for t in node.targets):nodes.append(node)
  if isinstance(node,ast.Expr)and isinstance(node.value,ast.Call)and isinstance(node.value.func,ast.Attribute)and isinstance(node.value.func.value,ast.Name)and node.value.func.value.id==target and node.value.func.attr=='update':nodes.append(node)
 assert nodes,name
 namespace=dict(Path=Path,sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest())
 exec(compile(ast.Module(body=nodes,type_ignores=[]),str(p/name),'exec'),namespace)
 actual=namespace[target];actual={str(f)for f in actual};assert actual==expected,(name,actual^expected)
 assert all(Path(f).is_file()for f in actual)
print('PASS build/run source manifests match and all source files are readable without services')
