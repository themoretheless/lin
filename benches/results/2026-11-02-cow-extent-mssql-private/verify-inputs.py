from pathlib import Path
import hashlib,json,re,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'input.json').read_text())
for n,h in m['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
for n,h in m['runtime_source_sha256'].items():assert sha(subprocess.check_output(['git','show',m['runtime_reference']+':'+n],cwd=root))==h and sha((root/n).read_bytes())==h,n
assert m['features']==['default','experimental-extent-wal']and m['upstream_correctness_run']=='41401044023'
workflow=(root/'.github/workflows/cow-extent-mssql-private.yml').read_text();prefix=str(p.relative_to(root));paths=re.findall(r'benches/results/[A-Za-z0-9_./-]+',workflow)
assert paths and all(x.rstrip('/')==prefix or x.startswith(prefix+'/')for x in paths)
for n in paths:
 if n.endswith('.py'):assert(root/n).is_file(),n
for f in p.glob('*.py'):compile(f.read_text(),str(f),'exec')
assert 'Upload encrypted'in workflow or 'cow-extent-mssql-private-${{ github.run_id }}'in workflow
assert 'lin-cow-extent-mssql-private.tar.gz.age'in workflow and 'age -R .github/benchmark-recipients/mssql.age'in workflow
assert 'scripts/bench-mssql-native-ci.py'in(p/'run.py').read_text()
assert "'--features','experimental-extent-wal'"in(p/'build.py').read_text()
print('Frozen current runtime, complete inputs, root entrypoint and private artifact paths verified')
