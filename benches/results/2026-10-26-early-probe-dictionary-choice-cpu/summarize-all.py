from pathlib import Path
import json,statistics as st,math,hashlib
root=Path(__file__).resolve().parents[3]
for dirname in ['2026-10-26-neon-dictionary-choice-cpu','2026-10-26-bounded-dictionary-choice-cpu','2026-10-26-early-probe-dictionary-choice-cpu']:
 p=root/'benches/results'/dirname; cases={};raw_hashes={}
 for n in range(1,13):
  f=p/f'process-{n}.jsonl'; raw_hashes[f.name]=hashlib.sha256(f.read_bytes()).hexdigest()
  for line in f.read_text().splitlines():
   r=json.loads(line);assert r['exact_bits'] and len(r['ns'])==3 and all(len(a)==24 for a in r['ns'])
   assert all(math.isfinite(v) and v>=0 for a in r['ns'] for v in a)
   qualified=all(v>0 for a in r['ns'] for v in a);a,b,c=map(st.median,r['ns'])
   cases.setdefault(r['case'],[]).append(dict(process=n,timing_qualified=qualified,baseline_ns=a,candidate_ns=c,gain_percent=100*(1-c/a) if qualified else None,aa_percent=100*(1-b/a) if qualified else None,reference_bytes=r['reference_bytes'],candidate_bytes=r['candidate_bytes'],selected=r['dictionary_selected']))
 out={}; table=[]
 for name,rs in cases.items():
  assert len(rs)==12;out[name]={};cells=[]
  for label,group in [('series1',rs[:6]),('series2',rs[6:])]:
   qualified=all(r['timing_qualified'] for r in group)
   q=dict(timing_qualified=qualified,gain_percent=st.median(r['gain_percent'] for r in group) if qualified else None,aa_absolute_percent=st.median(abs(r['aa_percent']) for r in group) if qualified else None,wins=sum(r['gain_percent']>0 for r in group) if qualified else None,processes=group);out[name][label]=q
   cells.append(f"{q['gain_percent']:+.2f}% / {q['wins']}/6 / {q['aa_absolute_percent']:.2f}%" if qualified else 'unqualified: zero-duration samples')
  table.append(f"| {name} | {' | '.join(cells)} | {rs[0]['reference_bytes']} / {rs[0]['candidate_bytes']} |")
 (p/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
 (p/'verification.json').write_text(json.dumps(dict(processes=12,cases_per_process=len(cases),samples_per_variant=24,all_exact_bits=True,raw_sha256=raw_hashes,unqualified_timing_cases=[k for k,v in out.items() if not all(q['timing_qualified'] for q in v.values())]),indent=2)+'\n')
 mechanism={'2026-10-26-neon-dictionary-choice-cpu':'NEON nonzero masks with four entropy probes. Rejected: unsampled high-cardinality majority triggers full candidate encoding then fallback, causing >80% regression. Special four-row timings include zeros and cannot support performance claims.', '2026-10-26-bounded-dictionary-choice-cpu':'Limits costly escapes to two and aborts early for dense rows/unsafe size bounds. The catastrophic high-cardinality fallback is bounded, but metadata/probing overhead remains. Special four-row timing uses 256 operations/sample and exposes a real routing regression.', '2026-10-26-early-probe-dictionary-choice-cpu':'Moves row-count/size hints before metadata scans; uses NEON probes and rejects obvious dense/zero controls early. Adds unsampled high/dense outliers at row998 (row999 was already probed). CPU target improves, but probe-miss fallback and tiny routing still regress. Not accepted as a default production writer.'}[dirname]
 (p/'report.md').write_text('# Dictionary encoder CPU experiment\n\n'+mechanism+'\n\nFrozen production reference e05ac74b (library source identical to a5e28e36). Twelve processes, two consecutive six-process series; rotated A/A/candidate with 24 samples per variant/case. Median process gain uses baseline A; A/A column is median absolute process deviation. No observations discarded. Buffers are preallocated/reused; allocation/drop, full WAL frame/CRC, file I/O, fsync, replay and peer runtimes are outside this measurement. Bytes below are vector sections only.\n\n| Case | Series1 gain / wins / A-A | Series2 gain / wins / A-A | Reference / candidate bytes |\n|---|---|---|---|\n'+'\n'.join(table)+'\n\nAll measured outputs were compared as raw f32 bits, including negative zero and NaNs, using a trusted diagnostic parser. Boundary dimensions65535/65536/65537 are correctness preflights. This parser is not production untrusted-input decoder qualification. Exact outputs and hashes do not establish end-to-end durability or superiority over SQLite.\n\nProduction Rust remains unchanged. Next gate: strict existing codec5 reader, mixed-vector budget, migration/old reader behavior, actual frame sizes and full-engine native/Full durable A/A/B plus dense/high-cardinality/cold/resource controls. Tiny-path and fallback regressions must remain visible; default enablement is not justified by this CPU result.\n')
