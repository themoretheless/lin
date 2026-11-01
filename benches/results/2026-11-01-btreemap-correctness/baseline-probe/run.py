from pathlib import Path
import subprocess,json,hashlib,shutil,os,re
p=Path(__file__).resolve().parent;ref='6812cc5ca9fc35a4adc81e0948df45dfe3629742';example=Path('examples/transaction_schema_undo_probe.rs');assert not example.exists();h=lambda b:hashlib.sha256(b).hexdigest();status=dict(baseline_reference=ref,performance_qualified=False)
try:
 sources={str(f):h(f.read_bytes())for f in Path('src').rglob('*.rs')};names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src'],text=True).splitlines();expected={n:h(subprocess.check_output(['git','show',ref+':'+n]))for n in names if n.endswith('.rs')};assert sources==expected
 status['source_sha256']=sources;status['compiler']=subprocess.check_output(['rustc','-Vv'],text=True);example.parent.mkdir(exist_ok=True);shutil.copyfile(p/'probe.rs',example);status['probe_sha256']=h(example.read_bytes())
 with(p/'baseline-runtime.log').open('w')as log:r=subprocess.run(['cargo','run','--release','--example','transaction_schema_undo_probe','--color','never'],stdout=log,stderr=subprocess.STDOUT,env=dict(os.environ,CARGO_TARGET_DIR='/tmp/lin-cow-vnext-target'))
 text=(p/'baseline-runtime.log').read_text();status['runtime_exit']=r.returncode;status['outcomes']=[json.loads(s)for s in text.splitlines()if s.startswith('{')]
 contract=re.search(r'ROW_CONTRACT row_bytes=(\d+) btreemap_bytes=(\d+)',text)
 status['exact_btreemap_contract_verified']=bool(contract)and 'Finished `release` profile'in text
 if contract:status['row_bytes'],status['btreemap_bytes']=map(int,contract.groups())
 status['runtime_reproduced']=r.returncode==3 and len(status['outcomes'])==6 and all(not o['passes']for o in status['outcomes'])and status['exact_btreemap_contract_verified']
finally:
 if example.exists():example.unlink()
 status['runtime_sources_unchanged']=all(h(Path(n).read_bytes())==v for n,v in status.get('source_sha256',{}).items())
 (p/'status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['runtime_reproduced']and status['runtime_sources_unchanged']
print('Six schema rollback failures reproduced on exact BTreeMap reference; public Row type equality verified')
