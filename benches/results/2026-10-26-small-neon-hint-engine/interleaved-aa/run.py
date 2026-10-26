from pathlib import Path
import subprocess,json,hashlib,os,statistics
p=Path(__file__).resolve().parent
meta=json.loads((p/'provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
for v,h in meta['binary_sha256'].items():assert hashlib.sha256(Path('/tmp/lin-small-neon-hint-interleaved-aa-'+v).read_bytes()).hexdigest()==h
summary=[]
for n in [1000,10000]:
 for pair in range(1,7):
  workers={};logs=[];samples={'baseline':[],'candidate':[]};events=[]
  try:
   for v in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
    log=(p/f'rows-{n}-pair-{pair}-{v}.log').open('w');logs.append(log)
    w=subprocess.Popen(['/tmp/lin-small-neon-hint-interleaved-aa-'+v,str(n)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,bufsize=1);workers[v]=w
    assert json.loads(w.stdout.readline())=={'ready':True,'rows':n}
   def request(v):
    w=workers[v];w.stdin.write('sample\n');w.stdin.flush();d=json.loads(w.stdout.readline());assert d['exact_bits'] and d['validated_rows']==n and d['ns']>0;return d
   for v in workers:request(v) # Preflight outside saved timing observations.
   for i in range(24):
    order=['baseline','candidate'] if (i+pair)%2 else ['candidate','baseline']
    for v in order:
     load=os.getloadavg();d=request(v);samples[v].append(d['ns']);events.append(dict(variant=v,round=i,load_before=load,**d))
   med={v:statistics.median(x) for v,x in samples.items()};summary.append({'rows':n,'pair':pair,'median_ns':med,'paired_gain_percent':100*(med['baseline']-med['candidate'])/med['baseline']})
   (p/f'rows-{n}-pair-{pair}.json').write_text(json.dumps({'samples':samples,'events':events,'exact_bits':True},indent=2))
  finally:
   for w in workers.values():
    w.stdin.close()
    try:rc=w.wait(timeout=30)
    except subprocess.TimeoutExpired:w.kill();w.wait();raise
    assert rc==0,rc
   for log in logs:log.close()
  (p/'summary.json').write_text(json.dumps(summary,indent=2));print(n,pair,round(summary[-1]['paired_gain_percent'],3),flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
(p/'verification.json').write_text(json.dumps({'process_pairs':12,'worker_exit_codes_zero':True,'source_unchanged':True,'measured_samples':576,'preflight_samples':24,'exact_bits':True},indent=2))
