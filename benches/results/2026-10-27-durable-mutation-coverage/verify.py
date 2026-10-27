from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
expected={f'compare/durable_{op}_1row_{size}/{peer}'for size in ['1k','10k']for op in ['update','delete','commit','rollback']for peer in ['lin','sqlite_delete','sqlite_wal']}
s=load('smoke-status.json');f=load('fault-status.json')
assert len(s['runs'])==len(f['runs'])==24
assert {r['case']for r in s['runs']}=={r['case']for r in f['runs']}==expected
assert s['correctness_only']and not s['performance_qualified']and f['all_faults_rejected']
for r in s['runs']:
 assert r['exit_code']==0
 d=load('smoke/'+r['case'].replace('/','_')+'/run.json')
 assert d['status']=='complete'and len(d['cases'])==1 and len(d['observations'])==1
 assert d['observations'][0]['case']==r['case']and d['observations'][0]['operations']==1
for r in f['runs']:
 assert r['exit_code']!=0 and r['output_bytes']>0 and len(r['output_sha256'])==64
 prefix=(p/'fault-check'/(r['case'].replace('/','_')+'.prefix.log')).read_bytes()
 assert b'full Lin values after reopen'in prefix or b'full SQLite values after reopen'in prefix
for name,source in [('provenance.json','compare.rs'),('provenance-fault.json','compare-fault.rs')]:
 m=load(name);assert m['source_restored']and hashlib.sha256((p/source).read_bytes()).hexdigest()==m['compiled_compare_sha256']
 for path,digest in m['source_sha256'].items():assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest,path
 assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==m['original_compare_sha256']
print('PASS: 24 positive cases; 24 operation faults rejected; sources restored; no performance qualification')
