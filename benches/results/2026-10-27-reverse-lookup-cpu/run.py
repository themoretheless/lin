from pathlib import Path
import subprocess,json,statistics as st,hashlib
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());assert hashlib.sha256(Path('/tmp/lin-reverse-lookup').read_bytes()).hexdigest()==meta['binary_sha256'];out={}
for i in range(1,7):
 with(p/f'process-{i}.jsonl').open('w')as f:subprocess.run(['/tmp/lin-reverse-lookup'],stdout=f,check=True)
 rows=[json.loads(s)for s in(p/f'process-{i}.jsonl').read_text().splitlines()];assert len(rows)==144 and all(r['exact_lookup']for r in rows)
 for n,kind in sorted({(r['rows'],r['kind'])for r in rows}):
  a=[r for r in rows if r['rows']==n and r['kind']==kind];v={k:st.median(r[k]for r in a)for k in ['baseline_ns','control_ns','candidate_ns']};v.update(process=i,gain=100*(1-v['candidate_ns']/v['baseline_ns']),aa=100*(1-v['control_ns']/v['baseline_ns']),saved_ns=v['baseline_ns']-v['candidate_ns'],dense_requested_bytes=a[0]['dense_requested_bytes']);out.setdefault(f'{n}/{kind}',[]).append(v)
summary={k:dict(gain_percent=st.median(r['gain']for r in a),aa_absolute_percent=st.median(abs(r['aa'])for r in a),saved_ns=st.median(r['saved_ns']for r in a),positive_pairs=sum(r['gain']>0 for r in a),processes=a)for k,a in out.items()}
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for k,v in summary.items():print(k,round(v['saved_ns'],3),round(v['gain_percent'],3),v['positive_pairs'])
