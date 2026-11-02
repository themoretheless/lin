from pathlib import Path
import hashlib,json,math,re,statistics,subprocess,sys,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2];run=41391257035;head='20a408f6cc930dc88f64251e737718716495c200';sha=lambda b:hashlib.sha256(b).hexdigest()
meta=json.loads(subprocess.check_output(['gh','run','view',str(run),'--json','databaseId,headSha,status,conclusion,url,jobs'],cwd=root,text=True))
assert meta['status']=='completed','keep polling the same live run'
assert meta['headSha']==head and meta['conclusion']=='success','preserve failed run separately; no partial acceptance'
raw=Path(f'/tmp/lin-extent-wal-component-{run}');assert not raw.exists(),'preserve existing download'
subprocess.run(['gh','run','download',str(run),'--name','extent-wal-component','--dir',str(raw)],cwd=root,check=True)
load=lambda name:json.loads((raw/name).read_text());inputs=load('input.json');m=load('provenance.json')
assert inputs==json.loads(subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root,text=True))
for name,h in inputs['files_sha256'].items():
 assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
 if name.startswith(str(p.relative_to(root))+'/'):assert sha((raw/Path(name).name).read_bytes())==h,name
assert m['source_restored']and m['runtime_reference']==inputs['runtime_reference']and m['source_and_tests_sha256']==inputs['source_and_tests_sha256']and m['source_commit']==head
assert 'release: 1.96.0'in m['rustc']and 'host: x86_64-unknown-linux-gnu'in m['rustc']
for name,h in inputs['source_and_tests_sha256'].items():assert sha(subprocess.check_output(['git','show',inputs['runtime_reference']+':'+name],cwd=root))==h,name
for mode,expected,ignored in [('feature',476,4),('default',463,2)]:
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/(mode+'-tests.log')).read_text());assert [sum(int(r[i])for r in counts)for i in range(3)]==[expected,0,ignored]
bins=Path(f'/tmp/lin-extent-wal-executable-{run}');assert not bins.exists()
subprocess.run(['gh','run','download',str(run),'--name','extent-wal-exact-linux-executable','--dir',str(bins)],cwd=root,check=True)
elf=(bins/Path(m['binary']['path']).name).read_bytes();assert elf[:6]==b'\x7fELF\x02\x01'and int.from_bytes(elf[18:20],'little')==62 and sha(elf)==m['binary']['sha256']
sys.path.insert(0,str(root/'scripts'))
from benchmark_role_balance import schedule,orders_from_records
cases=[f'compare/durable_{op}_1row_{n}/{engine}'for n in ['1k','10k']for op in ['update','delete','commit','rollback']for engine in ['lin','sqlite_wal']]
cases +=[f'compare/update_1row_{n}/lin'for n in ['1k','10k','100k']]+['compare/insert_bulk_10k/lin','compare/fts_lex_selective/lin'];roles=['base','cont','cand'];summaries={};peer_results=[];count=0
for series in [1,2]:
 q=raw/f'series-{series}';get=lambda name:json.loads((q/name).read_text());records=get('process-status.json');smokes=get('smoke-status.json')
 assert get('cases.json')==cases and len(records)==378 and len(smokes)==42
 assert get('status.json')==dict(processes=378,observations=3024,smokes=42,measurement_complete=True,source_unchanged=True,one_binary=True,performance_qualified=False,adopted=False)
 assert get('environment.json')['source_sha256']=={n:h for n,h in inputs['source_and_tests_sha256'].items()if n.startswith('src/')and n.endswith('.rs')}
 plan=schedule([dict(id=c)for c in cases],roles,rounds=6);assert [(r['round'],r['role'],r['case'])for r in records]==[(r['round'],r['role'],r['workload'])for r in plan]
 orders_from_records(records,cases,roles,'case',rounds=6)
 assert {(r['role'],r['case'])for r in smokes}=={(role,c)for role in ['base','cand']for c in cases}
 med={}
 for smoke,rows in [(True,smokes),(False,records)]:
  for r in rows:
   mode='extent'if r['role']=='cand'and '/durable_'in r['case']and r['case'].endswith('/lin')else'legacy';assert r['layout']==mode and r['exit_code']==0 and r['binary_sha256']==m['binary']['sha256']
   n=1 if smoke else 8;relative=f"smokes/{r['role']}-{cases.index(r['case']):02}"if smoke else f"raw/{r['round']:02}-{r['role']}/"+r['case'].replace('/','_')
   command=r['command'];assert command[:-1]==[m['binary']['path'],'--filter',r['case'],'--samples',str(n),'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output']
   assert command[-1].endswith('/'+str(p.relative_to(root))+f'/series-{series}/'+relative)
   d=json.loads((q/relative/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==n
   values=[]
   for o in d['observations']:
    assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available';v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
   median=statistics.median(values);assert median==r['median_ns']
   if not smoke:med[r['round'],r['role'],r['case']]=median;count+=len(values)
 rebuilt={}
 for case in cases:
  pairs=[]
  for rnd in range(1,7):
   a,b,c=(med[rnd,role,case]for role in roles);pairs.append(dict(round=rnd,base=a,cont=b,cand=c,gain_percent=100*(1-c/a),aa_percent=100*(1-b/a)))
  rebuilt[case]=dict(gain_percent=statistics.median(r['gain_percent']for r in pairs),aa_absolute_percent=statistics.median(abs(r['aa_percent'])for r in pairs),positive_pairs=sum(r['gain_percent']>0 for r in pairs),pairs=pairs)
 assert rebuilt==get('summary.json');summaries[str(series)]=rebuilt
 peers=[]
 for n in ['1k','10k']:
  for op in ['update','delete','commit','rollback']:
   lin=f'compare/durable_{op}_1row_{n}/lin';sql=f'compare/durable_{op}_1row_{n}/sqlite_wal'
   for rnd in range(1,7):peers.append(dict(rows=n,operation=op,round=rnd,lin_ns=med[rnd,'cand',lin],sqlite_ns=med[rnd,'base',sql],lin_over_sqlite=med[rnd,'cand',lin]/med[rnd,'base',sql]))
 audit=get('audit.json');assert audit==dict(audit_complete=True,processes=378,observations=3024,smokes=42,one_binary=True,raw_recomputed=True,sqlite_full_wal_comparisons=peers,sqlite_wins=sum(r['lin_over_sqlite']<1 for r in peers),sqlite_comparisons=48,performance_qualified=False,adopted=False)
 peer_results.append(dict(series=series,wins=audit['sqlite_wins'],comparisons=48,pairs=peers))
assert count==6048
out=root/'benches/results/2026-11-02-extent-wal-component-linux';out.mkdir(exist_ok=False);files=sorted(f for f in raw.rglob('*')if f.is_file());manifest={str(f.relative_to(raw)):dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in files}
with tarfile.open(out/'raw.tar.gz','w:gz')as t:
 for f in files:t.add(f,arcname=str(f.relative_to(raw)),recursive=False)
with tarfile.open(out/'raw.tar.gz')as t:
 assert sorted(t.getnames())==sorted(manifest)
 for member in t:assert sha(t.extractfile(member).read())==manifest[member.name]['sha256']
for name,value in [('raw-manifest.json',manifest),('run-metadata.json',meta),('summary.json',summaries),('sqlite-full-wal.json',peer_results),('independent-audit.json',dict(files=len(files),processes=756,observations=6048,smokes=84,one_binary=True,exact_elf_verified=True,elf_executed_locally=False,raw_recomputed=True,frozen_git_blobs_verified=True,sqlite_wins=sum(r['wins']for r in peer_results),sqlite_comparisons=96,full_matrix_qualified=False,adopted=False))]:
 (out/name).write_text(json.dumps(value,indent=2)+'\n')
lines=['# Same-binary extent WAL database component','','Two six-round balanced Linux series. Positive gain means lower candidate latency; AA is median absolute baseline/control variation.','','| Case | Gain S1 % | AA S1 % | Pairs S1 | Gain S2 % | AA S2 % | Pairs S2 |','|---|---:|---:|---:|---:|---:|---:|']
for case in cases:
 a,b=summaries['1'][case],summaries['2'][case];lines.append(f"| {case} | {a['gain_percent']:+.3f} | {a['aa_absolute_percent']:.3f} | {a['positive_pairs']}/6 | {b['gain_percent']:+.3f} | {b['aa_absolute_percent']:.3f} | {b['positive_pairs']}/6 |")
lines+=['',f"Matched SQLite FULL WAL comparisons: {sum(r['wins']for r in peer_results)}/96 wins. All 756 processes, 6048 observations, 84 smokes, immutable ELF and frozen Git blobs were independently checked.",'','This is a component experiment, not the full 67-case acceptance matrix. Setup/prewrite/verification costs are outside the inherited operation timing boundary and retained in driver elapsed times. All losses and control variation remain in the archive. No unchanged retry, adoption, power-loss qualification or full-objective completion follows. Later prepared-column/read-only/single-pass source changes are excluded from frozen commit 20a408f6c.']
(out/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(files=len(files),processes=756,observations=count,sqlite_wins=sum(r['wins']for r in peer_results),sqlite_comparisons=96,adopted=False)))
