from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent;old=p.parent/'2026-10-27-cow-broad-engine';s=json.loads((p/'status.json').read_text());assert s['source_restored']and s['production_api_unchanged']and not s['performance_qualified']
for f,h in s['source_sha256'].items():assert hashlib.sha256(Path(f).read_bytes()).hexdigest()==h,f
assert hashlib.sha256((p/'owning-tests.rs').read_bytes()).hexdigest()==s['tests_sha256']
frozen=(old/'candidate-store.rs').read_text();adapter=(p/'adapter.rs').read_text();assert (p/'candidate-store.rs').read_text()==frozen.replace('impl From<BTreeMap<String, Cell>> for Row {',adapter+'impl From<BTreeMap<String, Cell>> for Row {')
for n,h in s['candidate_sha256'].items():assert hashlib.sha256(((p if n=='store'else old)/f'candidate-{n}.rs').read_bytes()).hexdigest()==h
for role in ['baseline','candidate']:
 assert s['roles'][role]['exit_code']==0 and '6 passed; 0 failed;'in(p/f'{role}.log').read_text()
assert s['roles']['baseline']['implicit_map_exit_code']==0 and s['roles']['candidate']['implicit_map_exit_code']!=0 and 'E0308'in(p/'candidate-implicit.log').read_text()
assert json.loads((p/'test-status.json').read_text())['exit_code']==0
counts=[tuple(map(int,m))for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (p/'tests.log').read_text())]
assert counts and all(failed==0 for _,failed,_ in counts)
assert not Path('tests/cow_owning_adapters_probe.rs').exists()and not Path('examples/cow_implicit_map_probe.rs').exists()
print('PASS: owning adapters and implicit break; default suite',sum(v[0]for v in counts),'passed,',sum(v[2]for v in counts),'ignored; source restored; no performance qualification')
