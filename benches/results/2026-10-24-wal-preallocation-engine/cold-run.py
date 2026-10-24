from pathlib import Path
import subprocess,json,re,hashlib,statistics
p=Path(__file__).resolve().parent
meta=json.loads((p/'helper-provenance.json').read_text());out=[]
for pair in range(1,7):
 for v in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
  binary='/tmp/lin-preallocation-cold-'+v;assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==meta['binary_sha256']['cold-'+v]
  root=p/f'cold-p{pair}-{v}';assert not root.exists()
  with (p/f'cold-p{pair}-{v}.json').open('w') as stdout,(p/f'cold-p{pair}-{v}.log').open('w') as stderr:r=subprocess.run(['/usr/bin/time','-l',binary,str(root)],stdout=stdout,stderr=stderr)
  assert r.returncode==0
  d=json.loads((p/f'cold-p{pair}-{v}.json').read_text());assert d['exact_bits'] and not root.exists()
  d.update(pair=pair,variant=v,max_rss_bytes=int(re.search(r'(\d+)\s+maximum resident set size',(p/f'cold-p{pair}-{v}.log').read_text())[1]));out.append(d)
(p/'cold-observations.json').write_text(json.dumps(out,indent=2))
summary={}
for key in ['open_ns','commit_ns','open_to_commit_ns','drop_checkpoint_ns','max_rss_bytes','open_allocated_bytes','commit_allocated_bytes']:
 a=[next(d[key] for d in out if d['pair']==i and d['variant']=='baseline') for i in range(1,7)];b=[next(d[key] for d in out if d['pair']==i and d['variant']=='candidate') for i in range(1,7)]
 summary[key]={'baseline':statistics.median(a),'candidate':statistics.median(b),'paired_gain_percent':statistics.median(100*(x-y)/x for x,y in zip(a,b) if x) if any(a) else None}
(p/'cold-summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
