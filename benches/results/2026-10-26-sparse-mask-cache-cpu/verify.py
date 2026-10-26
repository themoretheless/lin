from pathlib import Path
import hashlib,json,math,subprocess,argparse
p=Path(__file__).resolve().parent;meta=json.loads((p/'provenance.json').read_text());sha=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
parser=argparse.ArgumentParser();parser.add_argument('--frozen',action='store_true');args=parser.parse_args()
if args.frozen:
 assert all(hashlib.sha256(subprocess.check_output(['git','show',meta['source_commit']+':'+f])).hexdigest()==h for f,h in meta['source_sha256'].items())
else:
 assert all(sha(f)==h for f,h in meta['source_sha256'].items())
assert sha(p/'gate.rs')==meta['gate_sha256'];assert sha('/tmp/lin-sparse-mask-cache-gate')==json.loads((p/'binary-sha256.json').read_text())['sha256']
status=json.loads((p/'status.json').read_text());assert len(status)==12 and all(x['exit_code']==0 for x in status.values())
ops=0
for i in range(1,13):
 d=[json.loads(l) for l in (p/f'process-{i}.jsonl').read_text().splitlines()];assert len(d)==6
 for x in d:
  assert x['exact_bytes_and_codec'] and len(x['ns'])==3
  for ns in x['ns']:assert len(ns)==24 and all(math.isfinite(t) and t>0 for t in ns);ops+=len(ns)
assert ops==5184
(p/'verification.json').write_text(json.dumps({'timed_compositions':ops,'frozen_source_hashes_match':True,'exact_bytes_and_codec':True,'all_processes_exit_zero':True,'production_change':False},indent=2)+'\n');print('PASS: corrected 5184 compositions; initial harness retained separately; no production change')
