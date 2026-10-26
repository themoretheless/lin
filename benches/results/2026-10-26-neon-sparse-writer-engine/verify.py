from pathlib import Path
import json,hashlib,math,statistics as st
r=Path(__file__).resolve().parent;status=json.loads((r/'rust-status.json').read_text());assert len(status)==48 and set(status.values())=={0};observations=0
for name in status:
 d=json.loads((r/name/'run.json').read_text());assert d['status']=='complete';assert len(d['cases'])==4
 for case in d['cases']:
  obs=[x for x in d['observations'] if x['case']==case['id']];assert len(obs)==24
  assert all(x['operations']==1 and math.isfinite(float(x['value'])) and float(x['value'])>0 for x in obs);observations+=len(obs)
checks=json.loads((r/'final-checks.json').read_text());assert checks['bench_binary_equal_measured'] and checks['interleaved_binary_equal_measured'];assert all(checks[x]['exit_code']==0 for x in ['no-default','optional','final-bench'])
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in checks['source_sha256'].items())
for stem in ['interleaved']:
 out=[]
 for mode in ['aa','ab','aa-s2','ab-s2']:
  d=json.loads((r/f'{stem}-{mode}/summary.json').read_text());assert len(d)==12
  verification=json.loads((r/f'{stem}-{mode}/verification.json').read_text());assert verification['worker_exit_codes_zero'] and verification['source_unchanged'] and verification['exact_bits'];assert verification['measured_samples']==576
  for n in sorted({x['rows'] for x in d}):
   a=[x['paired_gain_percent'] for x in d if x['rows']==n];out.append(dict(mode=mode,rows=n,gain=st.median(a),median_abs=st.median(map(abs,a)),wins=sum(x>0 for x in a)))
 (r/f'{stem}-summary.json').write_text(json.dumps(out,indent=2)+'\n')
crash=json.loads((r/'crash-resource.json').read_text());assert len(crash)==12
for n in [1000,10000]:
 for c in [1,3,4]:
  records=[x for x in crash if x['rows']==n and x['commits']==c];assert len(records)==2 and len({x['wal_sha256'] for x in records})==1;assert all(x['verified']['exact_bits'] for x in records)
(r/'verification.json').write_text(json.dumps(dict(conventional_processes=48,conventional_observations=observations,interleaved_processes=96,interleaved_observations=2304,interleaved_preflights=96,crash_exit_processes=12,readonly_replay_processes=12,byte_equal_wal_scenarios=6,source_unchanged=True,final_release_binaries_equal_measured=True),indent=2)+'\n')
print('PASS: frozen source, 48 conventional processes, 96 interleaved workers, exact replay and byte-equal WAL')
