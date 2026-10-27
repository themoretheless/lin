from pathlib import Path
import json,hashlib,re,math,sys
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
def median(xs):
 xs=sorted(xs);return (xs[(len(xs)-1)//2]+xs[len(xs)//2])/2
m=load('provenance.json');cases=load('cases.json');records=load('process-status.json');summary=load('summary.json');q=load('qualification.json');env=load('environment.json')
assert m['source_restored']and m['instrumentation']is False and m['source_sha256']
for f,h in m['source_sha256'].items():assert hashlib.sha256(Path(f).read_bytes()).hexdigest()==h
assert hashlib.sha256(Path(m['original_compare_path']).read_bytes()).hexdigest()==m['original_compare_sha256']
assert env['runtime_unchanged_all_roles']
assert len(cases)==len(set(cases))==24 and len(records)==576
assert {(r['round'],r['mode'],r['case'])for r in records}=={(i,mode,c)for i in range(1,13)for mode in ['baseline','control']for c in cases}
status=load('status.json');assert len(status)==24 and {r['run']for r in status}=={f'{i:02}-{mode}'for i in range(1,13)for mode in ['baseline','control']}
assert all(r['exit_code']==0 and r['subprocesses']==24 for r in status)
med={};rss={};count=0
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==m['binary']['sha256']and r['glibc_tunables']==env['allocator_environment']['GLIBC_TUNABLES']
 name=r['case'].replace('/','_');base=p/f'{r["round"]:02}-{r["mode"]}';d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8;values=[]
 for o in d['observations']:
  assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);count+=1
 k=r['round'],r['mode'],r['case'];med[k]=median(values);rss[k]=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1]);assert rss[k]==r['peak_rss_kib']>0
for c in cases:
 for label,start in [('series1',1),('series2',7)]:
  s=summary['cases'][c][label];assert s['aa_absolute_percent']==median(abs(100*(1-med[i,'control',c]/med[i,'baseline',c]))for i in range(start,start+6))
  for mode in ['baseline','control']:assert s['peak_rss_kib'][mode]==median(rss[i,mode,c]for i in range(start,start+6))
expected={(c,peer,mode,label)for c in cases if c.endswith('/lin')for peer in ['sqlite_delete','sqlite_wal']for mode in ['baseline','control']for label in ['series1','series2']}
rows=summary['comparisons'];assert len(rows)==64 and {(r['case'],r['peer'],r['mode'],r['series'])for r in rows}==expected
wins=0
for r in rows:
 c,peer,mode,label=r['case'],r['peer'],r['mode'],r['series'];sql=c[:-3]+peer;start=1 if label=='series1'else 7;rounds=range(start,start+6)
 ratios=[med[i,mode,c]/med[i,mode,sql]for i in rounds];w=sum(x<1 for x in ratios);wins+=w
 assert r['pairs']==6 and r['lin_wins']==w and r['median_ratio']==median(ratios)
 assert r['lin_ns']==median(med[i,mode,c]for i in rounds)and r['sqlite_ns']==median(med[i,mode,sql]for i in rounds)
 assert r['lin_aa_percent']==summary['cases'][c][label]['aa_absolute_percent']and r['sqlite_aa_percent']==summary['cases'][sql][label]['aa_absolute_percent']
 assert len(r['process_pairs'])==6
 for i,s in zip(rounds,r['process_pairs']):assert s==dict(round=i,lin_ns=med[i,mode,c],sqlite_ns=med[i,mode,sql],lin_over_sqlite=med[i,mode,c]/med[i,mode,sql])
assert count==4608 and q['processes']==576 and q['observations']==count and q['comparisons']==384 and q['lin_wins']==wins and q['measurement_complete']and not q['full_objective_achieved']and not q['source_candidate_qualified']
print('Independent audit PASS: 576 processes /4608 observations;',wins,'/384 peer wins; full objective false')
