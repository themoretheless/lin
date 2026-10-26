from pathlib import Path
import subprocess,json,hashlib,os,statistics,itertools
p=Path(__file__).resolve().parent;meta=json.loads((p/'interleaved-provenance.json').read_text());binary={k:'/tmp/lin-neon-sparse-count-interleaved-ab-'+v for k,v in [('a','baseline'),('control','baseline'),('b','candidate')]}
for k,v in [('a','baseline'),('control','baseline'),('b','candidate')]:assert hashlib.sha256(Path(binary[k]).read_bytes()).hexdigest()==meta[v]['binary_sha256']
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
q=p/'matched-controls';q.mkdir(exist_ok=True);(q/'provenance.json').write_text(json.dumps(dict(protocol='A/control/B in one block; A and control execute byte-identical baseline; six request orders rotate',binary_sha256={k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in binary.items()},source=meta),indent=2));summary=json.loads((q/'summary.json').read_text()) if (q/'summary.json').exists() else [];orders=list(itertools.permutations(binary))
for series in [1,2]:
 for n in [1000,10000]:
  for block in range(1,7):
   if any(x['series']==series and x['rows']==n and x['block']==block for x in summary):continue
   workers={};logs=[];samples={k:[] for k in binary};events=[]
   try:
    for k in orders[(series+block)%6]:
     log=(q/f's{series}-rows{n}-block{block}-{k}.log').open('w');logs.append(log)
     w=subprocess.Popen([binary[k],str(n)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,bufsize=1);workers[k]=w
     assert json.loads(w.stdout.readline())==dict(ready=True,rows=n)
    def request(k):
     w=workers[k];w.stdin.write('sample\n');w.stdin.flush();d=json.loads(w.stdout.readline());assert d['exact_bits'] and d['validated_rows']==n and d['ns']>0;return d
    for k in workers:request(k)
    for step in range(24):
     for k in orders[(step+series+block)%6]:
      load=os.getloadavg();d=request(k);samples[k].append(d['ns']);events.append(dict(label=k,round=step,load_before=load,**d))
    med={k:statistics.median(v) for k,v in samples.items()};reference=statistics.median([med['a'],med['control']]);summary.append(dict(series=series,rows=n,block=block,medians_ns=med,baseline_median_ns=reference,gain_percent=100*(reference-med['b'])/reference,aa_percent=100*(med['a']-med['control'])/med['a']))
    (q/f's{series}-rows{n}-block{block}.json').write_text(json.dumps(dict(samples=samples,events=events,exact_bits=True),indent=2))
   finally:
    for w in workers.values():
     w.stdin.close()
     try:rc=w.wait(timeout=30)
     except subprocess.TimeoutExpired:w.kill();w.wait();raise
     assert rc==0,rc
    for log in logs:log.close()
   (q/'summary.json').write_text(json.dumps(summary,indent=2));print(series,n,block,round(summary[-1]['gain_percent'],3),flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['candidate']['source_sha256'].items())
(q/'verification.json').write_text(json.dumps(dict(blocks=24,worker_processes=72,measured_operations=1728,preflights=72,source_unchanged=True,exact_bits=True,all_worker_exit_codes_zero=True),indent=2))
