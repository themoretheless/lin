from pathlib import Path
from itertools import permutations
import hashlib,json,math,statistics,subprocess,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
run='41398343205';head='dc337903d41faad2f927137e1ebc1ff2ebe0de08'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','status,conclusion,headSha,url'],cwd=root,text=True))
assert meta['status']=='completed' and meta['conclusion']=='success' and meta['headSha']==head,meta
raw=Path('/tmp/lin-index-reverse-reserve-'+run);base=Path('/tmp/lin-index-reverse-base-'+run);cand=Path('/tmp/lin-index-reverse-cand-'+run);out=root/'benches/results/2026-11-02-index-reverse-reserve-linux'
assert not any(f.exists()for f in [raw,base,cand,out]),'preserve prior attempt; explicitly revalidate instead of overwrite'
for name,dest in [('index-reverse-reserve',raw),('index-reverse-exact-linux-executables',base),('index-reverse-candidate-exact-linux-executable',cand)]:
 dest.mkdir();subprocess.run(['gh','run','download',run,'-n',name,'-D',str(dest)],cwd=root,check=True)
inputs=json.loads((raw/'input.json').read_text());assert(raw/'input.json').read_bytes()==subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root)
for name,h in inputs['files_sha256'].items():
 assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
 if name.startswith(str(p.relative_to(root))+'/'):assert sha((raw/Path(name).name).read_bytes())==h,name
build=json.loads((raw/'build-metadata.json').read_text());assert build['runtime_reference']==inputs['runtime_reference'];assert '1.96.0' in build['rustc']and'x86_64-unknown-linux-gnu'in build['rustc']
assert build['commands']==[dict(role=role,command=['cargo',action,'--release','-p','lin-peer-worker'])for role in ['base','cand']for action in ['test','build']]
for role,folder in [('base',base),('cand',cand)]:
 b=build['binaries'][role];binary=folder/('lin-index-reverse-'+role);assert sha(binary.read_bytes())==b['sha256'] and binary.read_bytes()[:4]==b'\x7fELF'
 assert '1 passed; 0 failed' in(raw/(role+'-test.log')).read_text()
roles=['base','cont','cand'];design=list(permutations(roles));expected=[(r,n,role)for r in range(1,7)for ci,n in enumerate([1000,10000,100000])for role in design[(r-1+ci)%6]]
all_summaries={}
for series in ['series1','series2']:
 d=raw/series;status=json.loads((d/'measure-status.json').read_text());obs=json.loads((d/'observations.json').read_text());summary=json.loads((d/'summary.json').read_text());manifest=json.loads((d/'raw-manifest.json').read_text())
 assert status['complete'] is True and status['peer_comparisons']==0 and status['processes']==54 and status['samples']==432 and status['exact_rows_verified']==1998000 and status['rustc']==build['rustc']
 for name,h in status['source_sha256'].items():assert inputs['files_sha256'][name]==h,name
 assert status['binaries_sha256']=={b['path']:b['sha256']for b in build['binaries'].values()}
 assert [(o['round'],o['rows'],o['role'])for o in obs]==expected
 assert [(c['round'],c['rows'],c['role'])for c in status['commands']]==expected
 assert set(manifest)=={f'{r}-{n}-{role}.{ext}'for r,n,role in expected for ext in ['json','log']}
 for name,h in manifest.items():assert sha((d/'raw'/name).read_bytes())==h,name
 checked=0
 for o,c in zip(obs,status['commands']):
  r,n,role=o['round'],o['rows'],o['role'];assert c['command']==[build['binaries']['cand'if role=='cand'else'base']['path']] and c['request']==dict(insert_rows=n,samples=8,timestamp_ms=1700000000000)
  response=json.loads((d/'raw'/f'{r}-{n}-{role}.json').read_text());v=response['samples_ns'];assert len(v)==8 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in v);assert v==o['samples_ns']and statistics.median(v)==o['median_ns'];assert len(response['result'])==n and response['statements']==(n+9999)//10000 and response['statement_rows_limit']==10000
  for i,row in enumerate(response['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',1700000000000,f'body {i}']
  checked+=n
 recomputed={}
 for n in [1000,10000,100000]:
  gains=[];aa=[]
  for r in range(1,7):
   v={o['role']:o['median_ns']for o in obs if o['round']==r and o['rows']==n};gains.append((1-v['cand']/v['base'])*100);aa.append(abs(v['cont']/v['base']-1)*100)
  recomputed[str(n)]=dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(x>0 for x in gains),gains_percent=gains)
 assert recomputed==summary and checked==1998000
 audit=dict(processes=54,samples=432,exact_rows_verified=checked,raw_members_verified=108,schedule_commands_binary_sources_and_summary_verified=True,performance_qualified=False,peer_comparisons=0);assert audit==json.loads((d/'independent-audit.json').read_text());all_summaries[series]=recomputed
manifest={str(f.relative_to(raw)):dict(size=f.stat().st_size,sha256=sha(f.read_bytes()))for f in sorted(raw.rglob('*'))if f.is_file()};out.mkdir()
with tarfile.open(out/'raw.tar.gz','w:gz')as archive:
 for name in manifest:archive.add(raw/name,arcname=name,recursive=False)
for name,data in [('raw-manifest.json',manifest),('run-metadata.json',meta),('summary.json',all_summaries),('independent-audit.json',dict(processes=108,samples=864,rows_verified=3996000,source_git_blobs_and_exact_elf_verified=True,summary_recomputed=True,peer_comparisons=0,adopted=False))]:(out/name).write_text(json.dumps(data,indent=2)+'\n')
lines=['# Reverse-map reservation Linux component','','Two balanced series; positive gains mean lower candidate latency. AA is baseline/control variation. No external engine was measured.','','| Rows | S1 gain % | S1 AA % | S1 positive | S2 gain % | S2 AA % | S2 positive |','|---|---:|---:|---:|---:|---:|---:|']
for n in ['1000','10000','100000']:
 a=all_summaries['series1'][n];b=all_summaries['series2'][n];lines.append(f"| {n} | {a['median_gain_percent']:+.3f} | {a['median_aa_percent']:.3f} | {a['positive_pairs']}/6 | {b['median_gain_percent']:+.3f} | {b['median_aa_percent']:.3f} | {b['positive_pairs']}/6 |")
lines+=['','All 108 processes, 864 observations and 3996000 six-field row readbacks independently checked. Full source blobs, baseline/control identity and both exact Linux ELFs verified. All losses and controls retained. This is component discovery, not full acceptance, peer superiority or adoption.']
(out/'report.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(files=len(manifest),processes=108,samples=864,adopted=False)))
