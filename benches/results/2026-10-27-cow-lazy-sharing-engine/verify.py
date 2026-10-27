from pathlib import Path
import json,hashlib,math,re
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());cases=json.loads((p/'cases.json').read_text());records=json.loads((p/'process-status.json').read_text());summary=json.loads((p/'summary.json').read_text());q=json.loads((p/'qualification.json').read_text())
assert len(cases)==len(set(cases))==37 and len(records)==1332
assert {(r['round'],r['mode'],r['case'])for r in records}=={(i,m,c)for i in range(1,13)for m in ['baseline','control','candidate']for c in cases}
assert meta['source_restored'] and meta['instrumentation']is False
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==meta['original_compare_sha256']
env=json.loads((p/'environment.json').read_text());assert env['runtime_unchanged_all_roles']
status=json.loads((p/'status.json').read_text());assert len(status)==36 and all(s['exit_code']==0 and s['subprocesses']==37 for s in status)
def median(xs):
 xs=sorted(xs);return (xs[(len(xs)-1)//2]+xs[len(xs)//2])/2
med={};count=0
for r in records:
 assert r['exit_code']==0 and r['glibc_tunables']==env['allocator_environment']['GLIBC_TUNABLES']
 assert r['binary_sha256']==meta['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
 name=r['case'].replace('/','_');base=p/f'{r["round"]:02}-{r["mode"]}';d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and len(d['observations'])==8;values=[]
 for o in d['observations']:
  assert o['case']==r['case'] and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);count+=1
 med[r['round'],r['mode'],r['case']]=median(values)
 assert int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1])==r['peak_rss_kib']>0
passed=True;wins=0;duckwins=0
for case in cases:
 for label,start in [('series1',1),('series2',7)]:
  gains=[];noise=[]
  for i in range(start,start+6):
   a,b,c=[med[i,m,case]for m in ['baseline','control','candidate']];gains.append(100*(1-c/a));noise.append(abs(100*(1-b/a)))
   if case.endswith('/lin'):
    wins+=c<med[i,'candidate',case[:-3]+'sqlite']
    if case[:-3]+'duckdb'in cases:duckwins+=c<med[i,'candidate',case[:-3]+'duckdb']
  gain,aa=median(gains),median(noise);ok=(all(g>0 for g in gains)and gain>aa)if case.startswith('compare/update_1row_')and case.endswith('/lin')else gain>=-max(2,aa)
  s=summary[case][label];assert s['gain_percent']==gain and s['aa_absolute_percent']==aa and s['passes']==ok;passed &=ok
assert count==10656 and q['source_candidate_qualified']==passed and q['candidate_sqlite_wins']==wins and q['candidate_sqlite_comparisons']==204 and q['full_objective_achieved']is False
assert json.loads((p/'test-status.json').read_text())['exit_code']==0
print('Independent audit PASS: 1332 processes, 10656 observations; source gate:',passed,'SQLite',wins,'/204; read DuckDB',duckwins,'/36')
