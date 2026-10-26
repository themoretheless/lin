from pathlib import Path
import subprocess,json,statistics as st,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());assert hashlib.sha256(Path('/tmp/lin-row-key-lifecycle').read_bytes()).hexdigest()==meta['binary_sha256'];out={}
for i in range(1,7):
 with(p/f'process-{i}.jsonl').open('w')as f,(p/f'process-{i}.log').open('w')as log:r=subprocess.run(['/tmp/lin-row-key-lifecycle'],stdout=f,stderr=log)
 assert r.returncode==0
 rows=[json.loads(s)for s in(p/f'process-{i}.jsonl').read_text().splitlines()];assert len(rows)==288 and all(r['exact_serde']for r in rows)
 for shape,copies in sorted({(r['shape'],r['copies'])for r in rows}):
  a=[r for r in rows if r['shape']==shape and r['copies']==copies];assert len(a)==24
  v={k:st.median(r[k]for r in a)for k in ['baseline_ns','control_ns','candidate_ns']};v.update(process=i,gain=100*(1-v['candidate_ns']/v['baseline_ns']),aa=100*(1-v['control_ns']/v['baseline_ns']),before_allocations=a[0]['before_allocations'],after_allocations=a[0]['after_allocations'],before_bytes=a[0]['before_bytes'],after_bytes=a[0]['after_bytes']);out.setdefault(shape+('/clonepatch'if copies else'/build'),[]).append(v)
 print('process',i,flush=True)
summary={k:dict(gain_percent=st.median(r['gain']for r in a),aa_absolute_percent=st.median(abs(r['aa'])for r in a),positive_pairs=sum(r['gain']>0 for r in a),processes=a)for k,a in out.items()}
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for k,v in summary.items():print(k,round(v['gain_percent'],3),round(v['aa_absolute_percent'],3),v['positive_pairs'])
