from pathlib import Path
import json,hashlib,math,re,sys
p=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
def median(xs):
 xs=sorted(xs);return (xs[(len(xs)-1)//2]+xs[len(xs)//2])/2
roles=['original','control','disabled','traced'];names=['exec_program','quota','encode','crc','write','sync','checkpoint','rollback'];m=load('provenance.json');cases=load('cases.json');records=load('process-status.json');s=load('summary.json');q=load('qualification.json');env=load('environment.json')
assert m['source_restored']and m['source_sha256']and not Path('src/mutation_profile.rs').exists()
for path,digest in m['source_sha256'].items():assert hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest
assert hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest()==m['original_compare_sha256']
for role,digest in m['benchmark_sha256'].items():assert hashlib.sha256((p/f'{role}-compare.rs').read_bytes()).hexdigest()==digest
for name,digest in m['instrumented_source_sha256'].items():assert hashlib.sha256((p/f'instrumented-{name}.rs').read_bytes()).hexdigest()==digest
assert hashlib.sha256((p/'profile.rs').read_bytes()).hexdigest()==m['profile_sha256']
assert env['all_roles']==roles and env['trace_enabled_roles']==['traced']and env['allocator_unchanged_all_roles']
assert len(cases)==len(set(cases))==24 and len(records)==576
assert {(r['round'],r['mode'],r['case'])for r in records}=={(i,role,c)for i in range(1,7)for role in roles for c in cases}
status=load('status.json');assert len(status)==24 and {r['run']for r in status}=={f'{i:02}-{role}'for i in range(1,7)for role in roles}and all(r['exit_code']==0 and r['subprocesses']==24 for r in status)
med={};rss={};traces={c:[]for c in cases if c.endswith('/lin')};nobs=0;nframes=0
for r in records:
 c,role,i=r['case'],r['mode'],r['round'];binary=m['binaries']['original'if role in ['original','control']else 'instrumented'];assert r['exit_code']==0 and r['binary_sha256']==binary['sha256']and r['glibc_tunables']==env['allocator_environment']['GLIBC_TUNABLES']and r['trace_enabled']==('1'if role=='traced'else '0')
 base=p/f'{i:02}-{role}';name=c.replace('/','_');d=json.loads((base/name/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==c and len(d['observations'])==8;values=[]
 for o in d['observations']:
  assert o['case']==c and o['metric']=='wall'and o['operations']==1
  v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v);nobs+=1
 med[i,role,c]=median(values);rss[i,role,c]=int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',(base/(name+'-resource.txt')).read_text())[1]);assert rss[i,role,c]==r['peak_rss_kib']>0
 path=base/(name+'.phases.json')
 if role!='traced':assert not path.exists()and 'trace_sha256'not in r;continue
 assert hashlib.sha256(path.read_bytes()).hexdigest()==r['trace_sha256'];t=json.loads(path.read_text());assert t['phase_names']==names and t['timing_qualified']is False and 'validation/calibration'in t['scope'];frames=t['records'];nframes+=len(frames)
 if not c.endswith('/lin'):assert not frames;continue
 assert len(frames)>=10;rollback='rollback_'in c
 for f in frames:
  assert isinstance(f['wall_ns'],int)and f['wall_ns']>0 and len(f['phase_ns'])==len(f['phase_calls'])==8
  assert all(isinstance(v,int)and v>=0 for v in f['phase_ns']+f['phase_calls'])and sum(f['phase_ns'])<=f['wall_ns']
  assert f['phase_calls'][0]==1 and f['phase_calls'][6]==0 and f['phase_calls'][7]==int(rollback)
  assert all(f['phase_calls'][j]==int(not rollback)for j in [2,3,4,5])and (f['payload_bytes']==0)==rollback
 traces[c].append(dict(round=i,frames=len(frames),whole_ns=median(f['wall_ns']for f in frames),payload_bytes=median(f['payload_bytes']for f in frames),phases={name:dict(ns=median(f['phase_ns'][j]for f in frames),share_percent=median(100*f['phase_ns'][j]/f['wall_ns']for f in frames),calls=sorted(set(f['phase_calls'][j]for f in frames)))for j,name in enumerate(names)}))
assert set(s['wall'])==set(cases)and set(s['phases'])==set(traces)
for c in cases:
 expected=dict(aa_absolute_percent=median(abs(100*(1-med[i,'control',c]/med[i,'original',c]))for i in range(1,7)),median_ns={role:median(med[i,role,c]for i in range(1,7))for role in roles},ratio_to_original={role:median(med[i,role,c]/med[i,'original',c]for i in range(1,7))for role in roles},traced_over_disabled=median(med[i,'traced',c]/med[i,'disabled',c]for i in range(1,7)),peak_rss_kib={role:median(rss[i,role,c]for i in range(1,7))for role in roles})
 assert s['wall'][c]==expected,c
for c,rows in traces.items():
 assert len(rows)==6 and {r['round']for r in rows}==set(range(1,7));expected=dict(frames=sum(r['frames']for r in rows),whole_ns=median(r['whole_ns']for r in rows),payload_bytes=median(r['payload_bytes']for r in rows),phases={n:dict(ns=median(r['phases'][n]['ns']for r in rows),share_percent=median(r['phases'][n]['share_percent']for r in rows))for n in names},processes=rows)
 assert s['phases'][c]==expected,c
peers=s['peers'];expected_keys={(c,peer,role)for c in traces for peer in ['sqlite_delete','sqlite_wal']for role in roles};assert len(peers)==64 and {(r['case'],r['peer'],r['mode'])for r in peers}==expected_keys
wins=0
for r in peers:
 c,sql,role=r['case'],r['case'][:-3]+r['peer'],r['mode'];ratios=[med[i,role,c]/med[i,role,sql]for i in range(1,7)];w=sum(v<1 for v in ratios);assert r['pairs']==6 and r['lin_wins']==w and r['median_ratio']==median(ratios)
 if role in ['original','control']:wins+=w
assert nobs==4608 and q['processes']==576 and q['observations']==nobs and q['trace_frames']==nframes and q['uninstrumented_peer_comparisons']==192 and q['uninstrumented_peer_wins']==wins and q['diagnostic_complete']and not q['source_candidate_qualified']and not q['full_objective_achieved']
assert load('test-status.json')==dict(exit_code=0,profile='release',features='default')
print('Independent audit PASS:576 processes,4608 wall observations,',nframes,'separate invocation frames; original peer wins',wins,'/192; no source acceptance')
