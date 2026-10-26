from pathlib import Path
import json,hashlib,statistics,collections
p=Path(__file__).resolve().parent
read=lambda f:json.loads((p/f).read_text())
sha=lambda f:hashlib.sha256((p/f).read_bytes()).hexdigest()
prov=read('provenance.json')
assert prov['baseline']['base_commit'].startswith('8b0e9717')
for v,name in [('baseline','before-exec.rs'),('candidate','candidate-exec.rs')]:
 assert sha(name)==prov[v]['source_sha256']['src/exec.rs']
assert sha('candidate-exec.rs')==sha('pre-postguard-final-candidate-exec.rs')
assert all(v==0 for v in read('postguard-checks.json').values())
assert sha('final-tested-exec.rs')==sha('final-candidate-exec.rs')
assert all(v==0 for v in read('checks.json').values())
final=read('final-checks.json');assert sha('final-candidate-exec.rs')==final['source_sha256']['src/exec.rs'];assert all(final[x]['exit_code']==0 for x in ['no-default','optional','final-bench'])
assert final['bench_binary_equal_measured'] and final['interleaved_binary_equal_measured']
assert len(read('rust-status.json'))==48 and all(v==0 for v in read('rust-status.json').values())
observations=0
for f in p.glob('rust-s*/run.json'):
 d=json.loads(f.read_text());assert d['status']=='complete';assert len(d['cases'])==4
 assert len(d['observations'])==96;observations+=len(d['observations'])
assert observations==4608
summary={};interleaved_samples=0
for family in ['interleaved','boundary','native','serialguard']:
 assert all(v==0 for v in read(f'{family}-status.json').values())
 for mode in (['aa-s3','ab-s3','aa-s4','ab-s4'] if family=='serialguard' else ['aa','ab','aa-s2','ab-s2']):
  folder=f'{family}-{mode}';data=read(folder+'/summary.json');assert len(data)==12
  ver=read(folder+'/verification.json');assert ver['source_unchanged'] and ver['worker_exit_codes_zero'] and ver['exact_bits']
  assert ver['measured_samples']==576;interleaved_samples+=576
  for n in sorted({x['rows'] for x in data}):
   values=[x['paired_gain_percent'] for x in data if x['rows']==n];assert len(values)==6
   for pair in range(1,7):
    raw=read(folder+f'/rows-{n}-pair-{pair}.json');assert raw['exact_bits']
    assert all(len(v)==24 for v in raw['samples'].values())
    med={k:statistics.median(v) for k,v in raw['samples'].items()}
    record=next(x for x in data if x['rows']==n and x['pair']==pair)
    assert med==record['median_ns']
    assert abs(record['paired_gain_percent']-100*(med['baseline']-med['candidate'])/med['baseline'])<1e-9
   summary[f'{folder}/{n}']={'paired_gain_percent':statistics.median(values),'median_absolute_difference_percent':statistics.median(map(abs,values)),'wins':sum(v>0 for v in values),'pair_values':values}
crash=read('crash-resource.json');assert len(crash)==24
for key in {(x['rows'],x['commits']) for x in crash}:
 a,b=[x for x in crash if (x['rows'],x['commits'])==key]
 assert a['wal_sha256']==b['wal_sha256'] and a['wal_bytes']==b['wal_bytes']
 for x in [a,b]:assert x['verified']==dict(verified_rows=x['rows']*x['commits'],commits=x['commits'],exact_bits=True)
(p/'all-summaries.json').write_text(json.dumps(summary,indent=2)+'\n')
(p/'verification.json').write_text(json.dumps(dict(snapshot_hashes=True,conventional_observations=observations,interleaved_observations=interleaved_samples,process_exit_writers=24,readonly_verifiers=24,byte_equal_wal_scenarios=12,decision='accepted for qualified small durable batches; full peer goal incomplete'),indent=2)+'\n')
print('PASS: frozen evidence internally consistent; no performance acceptance implied')
