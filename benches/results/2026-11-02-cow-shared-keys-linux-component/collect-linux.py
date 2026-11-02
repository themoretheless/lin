from pathlib import Path
import hashlib,json,math,re,statistics,subprocess,sys,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2];raw=Path('/tmp/lin-shared-keys-linux-41385469584')
sha=lambda b:hashlib.sha256(b).hexdigest();load=lambda name:json.loads((raw/name).read_text())
run=41385469584;head='62a2421117dc0653acfc4ac2525c92a3d1e9f12d'
metadata=json.loads(subprocess.check_output(['gh','run','view',str(run),'--json','databaseId,headSha,status,conclusion,url,jobs'],text=True,cwd=root))
assert metadata['status']=='completed'and metadata['conclusion']=='success'and metadata['headSha']==head
inputs=load('input.json');m=load('provenance.json')
assert inputs==json.loads(subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root,text=True))
for name,h in inputs['files_sha256'].items():
 assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
 if name.startswith(str(p.relative_to(root))+'/'):assert sha((raw/Path(name).name).read_bytes())==h,name
assert m['source_restored']and not m['performance_qualified']and 'release: 1.96.0'in m['rustc']
assert sha((raw/'compare.rs').read_bytes())==m['harness_sha256']==inputs['harness_sha256']
for role,expected in [('baseline',458),('candidate',462)]:
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/f'{role}-tests.log').read_text())
 assert [sum(int(r[i])for r in counts)for i in range(3)]==[expected,0,2]
 assert m['roles'][role]['commit']==inputs[role]['commit']and m['roles'][role]['source_and_tests_sha256']==inputs[role]['source_and_tests_sha256']
 assert m['roles'][role]['qa_exit']==m['roles'][role]['build_exit']==0
 for name,h in inputs[role]['source_and_tests_sha256'].items():assert sha(subprocess.check_output(['git','show',inputs[role]['commit']+':'+name],cwd=root))==h
assert sys.argv[1:] in ([],['--resume'])
binaries=Path('/tmp/lin-shared-keys-linux-executables-41385469584')
if binaries.exists():assert sys.argv[1:]==['--resume'],'preserve executable download; explicit resume revalidates all bytes'
else:subprocess.run(['gh','run','download',str(run),'--name','cow-shared-keys-exact-linux-executables','--dir',str(binaries)],cwd=root,check=True)
for role in ['baseline','candidate']:
 data=(binaries/Path(m[role]['path']).name).read_bytes();assert data[:6]==b'\x7fELF\x02\x01'and int.from_bytes(data[18:20],'little')==62 and sha(data)==m[role]['sha256']
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
summaries={};total=0
for series in [1,2]:
 q=raw/f'series-{series}';get=lambda name:json.loads((q/name).read_text());cases=get('cases.json');records=get('process-status.json');smokes=get('smoke-status.json');reported=get('summary.json')
 assert len(cases)==12 and len(set(cases))==12 and len(records)==216 and len(smokes)==24
 assert get('independent-audit.json')['audit_complete']
 assert get('status.json')==dict(processes=216,observations=1728,smokes=24,measurement_complete=True,source_restored=True,performance_qualification=False,adopted=False)
 original=get('environment.json')['source_sha256'];assert original=={n:h for n,h in inputs['candidate']['source_and_tests_sha256'].items()if n.startswith('src/')and n.endswith('.rs')}
 plan=schedule([dict(id=c)for c in cases],['baseline','control','candidate'],rounds=6)
 assert [(r['round'],r['role'],r['case'])for r in records]==[(r['round'],r['role'],r['workload'])for r in plan]
 orders_from_records(records,cases,['baseline','control','candidate'],'case',rounds=6)
 assert {(r['role'],r['case'])for r in smokes}=={(role,c)for role in ['baseline','candidate']for c in cases}
 med={}
 for smoke,rows in [(True,smokes),(False,records)]:
  for r in rows:
   binary=m['candidate'if r['role']=='candidate'and not r['case'].endswith('/duckdb')else'baseline'];n=1 if smoke else 8
   assert r['exit_code']==0 and r['binary_sha256']==binary['sha256']
   relative=f"smokes/{r['role']}-{cases.index(r['case']):02}"if smoke else f"raw/{r['round']:02}-{r['role']}/"+r['case'].replace('/','_')
   c=r['command'];assert c[:-1]==[binary['path'],'--filter',r['case'],'--samples',str(n),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output']
   assert c[-1].endswith('/'+str(p.relative_to(root))+f'/series-{series}/'+relative)
   d=json.loads((q/relative/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==n
   values=[]
   for o in d['observations']:
    assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'
    v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
   value=statistics.median(values);assert value==r['median_ns']
   if not smoke:med[r['round'],r['role'],r['case']]=value;total+=len(values)
 rebuilt={}
 for case in cases:
  pairs=[]
  for rnd in range(1,7):
   a,b,c=(med[rnd,role,case]for role in ['baseline','control','candidate'])
   pairs.append(dict(round=rnd,baseline=a,control=b,candidate=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
  rebuilt[case]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),aa_absolute_percent=statistics.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
 assert rebuilt==reported;summaries[str(series)]=rebuilt
assert total==3456
out=root/'benches/results/2026-11-02-cow-shared-keys-linux';out.mkdir(exist_ok=False)
files=sorted(f for f in raw.rglob('*')if f.is_file());manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
with tarfile.open(out/'raw.tar.gz','w:gz')as t:
 for f in files:t.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(out/'raw.tar.gz')as t:
 assert sorted(t.getnames())==sorted(manifest)
 for member in t:assert sha(t.extractfile(member).read())==manifest[member.name]['sha256']
for name,d in [('raw-manifest.json',manifest),('run-metadata.json',metadata),('summary.json',summaries),('independent-audit.json',dict(files=len(files),processes=432,observations=3456,smokes=48,exact_elf_hashes_verified=True,elf_executed_locally=False,raw_numerical_recompute_identical=True,frozen_git_blobs_verified=True,full_matrix_qualified=False,adopted=False))]:
 (out/name).write_text(json.dumps(d,indent=2)+'\n')
lines=['# Shared row keys: dedicated Linux component results','','Twelve cases, two balanced six-round series; 432 processes, 3456 observations, 48 smokes. Positive gain means lower latency. This is not full-matrix performance acceptance.','','| Case | Gain S1 % | AA S1 % | Pairs S1 | Gain S2 % | AA S2 % | Pairs S2 |','|---|---:|---:|---:|---:|---:|---:|']
for case in summaries['1']:
 a,b=summaries['1'][case],summaries['2'][case];lines.append(f"| {case} | {a['gain_percent']:+.3f} | {a['aa_absolute_percent']:.3f} | {a['positive_pairs']}/6 | {b['gain_percent']:+.3f} | {b['aa_absolute_percent']:.3f} | {b['positive_pairs']}/6 |")
lines+=['','Bulk INSERT 10k and rollback 100k are positive in all six pairs in both series and exceed the median absolute AA variation. Regressions remain: UPDATE 1k S1 and FULL UPDATE 10k S1 exceed AA; FULL COMMIT 1k S2 is -4.494% against AA 0.957%, with zero positive pairs. Candidate not adopted; no unchanged retry or control waiver.','','Both binaries use the exact same adapted harness and compiler. DuckDB uses the exact baseline executable for every role. Original source/test trees, full correctness counts, all raw data and exact ELF hashes were independently verified. These data do not prove wins over the named peer engines or resolve SQLite FULL WAL.']
(out/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(files=len(files),processes=432,observations=total,adopted=False,report=str(out/'report.md'))))
