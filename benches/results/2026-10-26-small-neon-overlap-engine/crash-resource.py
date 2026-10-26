from pathlib import Path
import subprocess,json,tempfile,hashlib,re,time
r=Path(__file__).resolve().parent;out=[];meta=json.loads((r/'interleaved-provenance.json').read_text())
for n in [512,1000,10000]:
 for commits in [1,3,4]:
  hashes={}
  for v in ['baseline','candidate']:
   binary='/tmp/lin-small-neon-overlap-interleaved-ab-'+v
   assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==meta[v]['binary_sha256']
   with tempfile.TemporaryDirectory(prefix='lin-persistent-crash-') as directory:
    name=f'crash-{n}-{commits}-{v}'
    with (r/(name+'.log')).open('w') as log:
     p=subprocess.run(['/usr/bin/time','-l',binary,'crash',directory,str(n),str(commits)],stdout=log,stderr=subprocess.STDOUT)
    assert p.returncode==0
    stat=(r/(name+'.log')).read_text();rss=int(re.search(r'(\d+)\s+maximum resident set size',stat)[1])
    wal=Path(directory,'log').read_bytes();hashes[v]=hashlib.sha256(wal).hexdigest()
    p=subprocess.run([binary,'verify',directory,str(n),str(commits)],capture_output=True,text=True)
    (r/(name+'-replay.log')).write_text(p.stdout+p.stderr);assert p.returncode==0
    verified=json.loads(p.stdout);assert verified=={'verified_rows':n*commits,'commits':commits,'exact_bits':True}
    out.append(dict(rows=n,commits=commits,variant=v,wal_bytes=len(wal),wal_sha256=hashes[v],max_rss_bytes=rss,verified=verified))
  assert hashes['baseline']==hashes['candidate'],(n,commits,hashes)
  (r/'crash-resource.json').write_text(json.dumps(out,indent=2));print(n,commits,'exact replay and byte-equal WAL',flush=True)
