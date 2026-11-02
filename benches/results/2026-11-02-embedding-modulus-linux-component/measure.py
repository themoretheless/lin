from pathlib import Path
import hashlib,json,math,os,platform,statistics,subprocess,sys,tarfile
protocol=Path(__file__).resolve().parent;root=protocol.parents[2];p=protocol/os.environ['SERIES'];assert not p.exists();p.mkdir();sha=lambda b:hashlib.sha256(b).hexdigest()
base=Path('/tmp/lin-embedding-modulus-base');cand=Path('/tmp/lin-embedding-modulus-cand')
assert not(p/'measure-status.json').exists()and not(p/'raw').exists(),'preserve prior attempt'
inputs=json.loads((protocol/'input.json').read_text());metadata=json.loads((protocol/'build-metadata.json').read_text())
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
for role,f in [('base',base),('cand',cand)]:assert f.is_file() and sha(f.read_bytes())==metadata['binaries'][role]['sha256']
assert subprocess.check_output(['git','diff','--name-only','60e64559ec0e5a21efd490d6da5a744e3ac8d0e2','--','src'],cwd=root,text=True).splitlines()==['src/embed.rs']
baseline_tree=Path('/tmp/lin-embedding-modulus-baseline')
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=baseline_tree,text=True).strip()==subprocess.check_output(['git','rev-parse','60e64559ec0e5a21efd490d6da5a744e3ac8d0e2'],cwd=root,text=True).strip()
assert not subprocess.check_output(['git','diff','--name-only','HEAD'],cwd=baseline_tree,text=True).strip()
input_files=[f for folder in ['src','lin-derive/src','tools/peer-worker/src']for f in(root/folder).rglob('*')if f.is_file()]+[root/n for n in ['Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','tools/peer-worker/Cargo.toml','examples/peer_bench.rs']]
for f in input_files:
 if str(f.relative_to(root))!='src/embed.rs':assert f.read_bytes()==(baseline_tree/f.relative_to(root)).read_bytes(),str(f)
source={str(f.relative_to(root)):sha(f.read_bytes())for f in input_files};binaries={str(f):sha(f.read_bytes())for f in[base,cand]}
sys.path.insert(0,str(root/'scripts'));from benchmark_role_balance import role_order,validate_orders
cases=[1000,10000,100000];orders={str(n):[role_order(r,ci,['base','cont','cand'])for r in range(1,7)]for ci,n in enumerate(cases)};validate_orders(orders,['base','cont','cand'])
(p/'raw').mkdir();observations=[];commands=[]
for rnd in range(1,7):
 for ci,n in enumerate(cases):
  for role in orders[str(n)][rnd-1]:
   binary=cand if role=='cand'else base;request=dict(insert_rows=n,samples=8,timestamp_ms=1700000000000);command=[str(binary)]
   response=subprocess.run(command,input=json.dumps(request)+'\n',text=True,capture_output=True,cwd=root);prefix=p/'raw'/f'{rnd}-{n}-{role}'
   prefix.with_suffix('.json').write_text(response.stdout);prefix.with_suffix('.log').write_text(response.stderr);assert response.returncode==0
   d=json.loads(response.stdout);assert len(d['samples_ns'])==8 and all(type(x)in(int,float)and math.isfinite(x)and x>0 for x in d['samples_ns']);assert len(d['result'])==n and d['statements']==(n+9999)//10000 and d['statement_rows_limit']==10000
   for i,row in enumerate(d['result']):assert row==[f'd-{i}',f'bench://{i}','rag'if i%2==0 else'sys',f'doc {i} wal note'if i%10==0 else f'doc {i} plain',1700000000000,f'body {i}']
   observations.append(dict(round=rnd,rows=n,role=role,median_ns=statistics.median(d['samples_ns']),samples_ns=d['samples_ns']));commands.append(dict(round=rnd,rows=n,role=role,command=command,request=request))
assert source=={str(f.relative_to(root)):sha(f.read_bytes())for f in input_files}
assert binaries=={str(f):sha(f.read_bytes())for f in[base,cand]}
for name,h in inputs['files_sha256'].items():assert sha((root/name).read_bytes())==h,name
summary={}
for n in cases:
 gains=[];aa=[]
 for rnd in range(1,7):
  vals={r['role']:r['median_ns']for r in observations if r['rows']==n and r['round']==rnd};gains.append((1-vals['cand']/vals['base'])*100);aa.append(abs(vals['cont']/vals['base']-1)*100)
 summary[n]=dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(x>0 for x in gains),gains_percent=gains)
(p/'observations.json').write_text(json.dumps(observations,indent=2)+'\n');(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
manifest={str(f.relative_to(p/'raw')):sha(f.read_bytes())for f in sorted((p/'raw').iterdir())}
with tarfile.open(p/'raw.tar.gz','w:gz')as archive:
 for name in manifest:archive.add(p/'raw'/name,arcname=name,recursive=False)
(p/'raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(p/'measure-status.json').write_text(json.dumps(dict(complete=True,exploratory_only=True,peer_comparisons=0,processes=54,samples=432,exact_rows_verified=1998000,platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),source_sha256=source,binaries_sha256=binaries,commands=commands),indent=2)+'\n');print(json.dumps(summary,indent=2))
