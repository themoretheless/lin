from pathlib import Path
import hashlib,json,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
m=json.loads((p/'input.json').read_text());sha=lambda b:hashlib.sha256(b).hexdigest()
def current():return {str(f.relative_to(root)):sha(f.read_bytes())for f in(root/'src').rglob('*.rs')}
def blob(ref,name):return subprocess.check_output(['git','show',ref+':'+name],cwd=root)
def install(ref,expected):
 data={n:blob(ref,n)for n in expected};assert {n:sha(b)for n,b in data.items()}==expected
 for f in(root/'src').rglob('*.rs'):
  if str(f.relative_to(root))not in data:f.unlink()
 for n,b in data.items():(root/n).write_bytes(b)
 assert current()==expected
state=p/'source-overlay.json'
if sys.argv[1:]==['--verify-inputs']:
 for ref,expected in [(m['runtime_reference'],m['runtime_source_sha256']),('HEAD',m['restore_source_sha256'])]:
  assert {n:sha(blob(ref,n))for n in expected}==expected
 print('PASS exact runtime and original source Git blobs');raise SystemExit(0)
if sys.argv[1:]==['--restore']:
 if not state.exists():print('No overlay created');raise SystemExit(0)
 old=json.loads(state.read_text());assert old['original_source_sha256']==m['restore_source_sha256']
 assert current()in [m['runtime_source_sha256'],old['original_source_sha256']],'Preserve unexpected source edits'
 install(old['checkout_commit'],old['original_source_sha256'])
 old['restored']=True;state.write_text(json.dumps(old,indent=2)+'\n');print('PASS original checkout source restored byte-for-byte');raise SystemExit(0)
assert not sys.argv[1:]and not state.exists(),'retain earlier attempt'
assert current()==m['restore_source_sha256']
ref=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
assert {n:sha(blob(ref,n))for n in m['restore_source_sha256']}==m['restore_source_sha256']
state.write_text(json.dumps(dict(checkout_commit=ref,original_source_sha256=m['restore_source_sha256'],runtime_reference=m['runtime_reference'],runtime_source_sha256=m['runtime_source_sha256'],restored=False),indent=2)+'\n')
install(m['runtime_reference'],m['runtime_source_sha256']);print('PASS exact625 runtime source installed, original Git blobs retained')
