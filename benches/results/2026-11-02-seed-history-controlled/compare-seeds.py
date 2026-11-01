from pathlib import Path
import subprocess,json,hashlib,os,struct
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'provenance.json').read_text());anchor=json.loads((p/'timestamp-anchor.json').read_text())['fixed_timestamp_ms'];assert m['source_restored']and m['single_binary_for_all_modes']and m['harness_sha256']==sha((p/'compare.rs').read_bytes())and sha(Path(m['binary']).read_bytes())==m['binary_sha256'];root=p/'seed-equivalence';root.mkdir(exist_ok=False);results=[]
for n in [1000,10000]:
 data={};generations={};files={}
 for mode in ['chunked','grouped']:
  target=root/f'{n}-{mode}';env=dict(os.environ,LIN_COLUMN_BATCH_FIXTURE=str(target));env.pop('LIN_BENCH_MIXED_CASE',None);cmd=[m['binary'],'--seed-history-case',str(n),'update','lin',mode];r=subprocess.run(cmd,env=env,capture_output=True,text=True);(root/f'{n}-{mode}.stdout').write_text(r.stdout);(root/f'{n}-{mode}.stderr').write_text(r.stderr);assert r.returncode==0 and 'LIN_IO_FULL_VALUES_REOPEN_OK'in r.stdout
  rows=json.loads((target/'expected.json').read_text());assert len(rows)==n and len({row['id']for row in rows})==n
  for row in rows:
   i=int(row['id'].split('-',1)[1]);assert row['ts']==dict(t='Time',v=anchor-(86400000 if i%2==0 else 30*86400000)), 'fixed timestamp is not in actual compiled fixture'
  data[mode]=rows;generations[mode]=int((target/'expected-generation.txt').read_text());files[mode]={f.name:dict(bytes=f.stat().st_size,sha256=sha(f.read_bytes()))for f in target.iterdir()if f.is_file()}
 assert data['chunked']==data['grouped'],'full seed values differ'
 for a,b in zip(data['chunked'],data['grouped']):
  assert a['embedding']['t']==b['embedding']['t']=='Vec'
  x=a['embedding']['v'];y=b['embedding']['v'];assert len(x)==len(y)
  assert all(struct.pack('>f',left)==struct.pack('>f',right)for left,right in zip(x,y)), 'embedding bits differ'
 assert generations['chunked']>generations['grouped']
 results.append(dict(rows=n,full_seed_values_equal=True,embedding_f32_bits_equal=True,fixed_timestamp_verified=True,generations=generations,files=files));print(n,'full seed values and f32 bits equal; timestamp fixed',flush=True)
(root/'status.json').write_text(json.dumps(dict(complete=True,cases=results,single_binary_sha256=m['binary_sha256'],performance_qualification=False,full_objective_achieved=False),indent=2)+'\n')
