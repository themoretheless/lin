from pathlib import Path
import json,math,statistics as st,re,sys,hashlib
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
cases=[f'compare/durable_{op}_1row_{size}/lin'for size in ['1k','10k']for op in ['update','delete','commit','rollback']]+['compare/wal_scalar_full_update/lin','compare/wal_scalar_normal_update/lin']
roles=['original','control','disabled','traced'];names=['exec_program','quota','encode','crc','write','sync','checkpoint','rollback']
records=json.loads((p/'process-status.json').read_text());m=json.loads((p/'provenance.json').read_text());assert m['source_restored']
assert len(records)==240 and {(r['round'],r['mode'],r['case'])for r in records}=={(i,role,c)for i in range(1,7)for role in roles for c in cases}
med={};rss={};phases={};observations=0;trace_hashes={}
for r in records:
 assert r['exit_code']==0 and r['binary_sha256']==m['binaries']['original'if r['mode']in ['original','control']else'instrumented']['sha256']
 base=p/f"{r['round']:02}-{r['mode']}";name=r['case'].replace('/','_');d=json.loads((base/name/'run.json').read_text())
 assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['case']and len(d['observations'])==8
 values=[]
 for o in d['observations']:
  v=float(o['value']);assert o['case']==r['case']and o['operations']==1 and o['metric']=='wall'and math.isfinite(v)and v>0;values.append(v);observations+=1
 key=r['round'],r['mode'],r['case'];med[key]=st.median(values)
 match=re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text());assert match and int(match[1])==r['peak_rss_kib']>0;rss[key]=r['peak_rss_kib']
 trace=base/(name+'-trace.json')
 if r['mode']!='traced':assert not trace.exists();continue
 data=json.loads(trace.read_text());assert data['phase_names']==names and data['timing_qualified'] is False and data['records']
 trace_hashes[str(trace.relative_to(p))]=hashlib.sha256(trace.read_bytes()).hexdigest()
 frames=data['records'];rollback='rollback'in r['case'];normal='scalar_normal'in r['case']
 for frame in frames:
  assert isinstance(frame['wall_ns'],int)and frame['wall_ns']>0 and len(frame['phase_calls'])==8 and len(frame['phase_ns'])==8
  assert all(isinstance(x,int)and x>=0 for x in frame['phase_calls']+frame['phase_ns'])
  for i in [2,3,4]:assert frame['phase_calls'][i]==int(not rollback)
  assert frame['phase_calls'][5]==int(not rollback and not normal) and frame['phase_calls'][6]==0 and frame['phase_calls'][7]==int(rollback)
  assert (frame['payload_bytes']==0)==rollback and frame['payload_bytes']>=0
 phases[r['round'],r['case']]=dict(records=len(frames),whole_ns=st.median(x['wall_ns']for x in frames),payload_bytes=st.median(x['payload_bytes']for x in frames),phase_ns={phase:st.median(x['phase_ns'][i]for x in frames)for i,phase in enumerate(names)},same_invocation_phase_percent={phase:st.median(100*x['phase_ns'][i]/x['wall_ns']for x in frames)for i,phase in enumerate(names)})
assert observations==1920 and len(phases)==60
summary={}
for case in cases:
 rounds=range(1,7)
 summary[case]=dict(wall_ns={role:st.median(med[i,role,case]for i in rounds)for role in roles},aa_absolute_percent=st.median(abs(100*(med[i,'control',case]/med[i,'original',case]-1))for i in rounds),disabled_over_original_ratio=st.median(med[i,'disabled',case]/med[i,'original',case]for i in rounds),traced_over_disabled_ratio=st.median(med[i,'traced',case]/med[i,'disabled',case]for i in rounds),peak_rss_kib={role:st.median(rss[i,role,case]for i in rounds)for role in roles},trace_records=sum(phases[i,case]['records']for i in rounds),traced_whole_work_ns=st.median(phases[i,case]['whole_ns']for i in rounds),payload_bytes=st.median(phases[i,case]['payload_bytes']for i in rounds),phase_ns={phase:st.median(phases[i,case]['phase_ns'][phase]for i in rounds)for phase in names},same_invocation_phase_percent={phase:st.median(phases[i,case]['same_invocation_phase_percent'][phase]for i in rounds)for phase in names})
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'trace-manifest.json').write_text(json.dumps(trace_hashes,indent=2)+'\n')
(p/'qualification.json').write_text(json.dumps(dict(diagnostic_complete=True,processes=240,observations=1920,source_candidate_qualified=False,full_objective_achieved=False,scope='Current COW caller-thread diagnostic only; trace invocations include calibration/validation and are not measurement-aligned'),indent=2)+'\n')
lines=['# Current COW durable caller-thread phase diagnostic','','All 240 fresh processes /1920 measured wall observations retained. Four roles: original/control share one binary; disabled/traced share the instrumented binary. Phase fractions are same-invocation ratios, aggregated within then across processes. Trace records include validation/calibration; no per-observation alignment is claimed. RSS includes fixture, oracle, reopen and tracing. Checkpoint is an inclusive parent; no nested totals are summed. No source optimization or peer superiority is qualified.','','|Case|A/A %|Disabled/original|Traced/disabled|Trace records|Payload bytes|Exec %|Encode %|Write %|Sync %|Rollback %|','|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|']
for case,r in summary.items():
 f=r['same_invocation_phase_percent'];lines.append(f"|{case}|{r['aa_absolute_percent']:.3f}|{r['disabled_over_original_ratio']:.3f}|{r['traced_over_disabled_ratio']:.3f}|{r['trace_records']}|{r['payload_bytes']}|{f['exec_program']:.3f}|{f['encode']:.3f}|{f['write']:.3f}|{f['sync']:.3f}|{f['rollback']:.3f}|")
(p/'report.md').write_text('\n'.join(lines)+'\n');print('Complete current-source diagnostic, 240 processes /1920 wall observations')
