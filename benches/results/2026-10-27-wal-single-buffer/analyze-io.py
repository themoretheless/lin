from pathlib import Path
import json,hashlib,subprocess,math,statistics
p=Path(__file__).resolve().parent
load=lambda n:json.loads((p/n).read_text())
h=lambda b:hashlib.sha256(b).hexdigest()
m=load('io-provenance.json');rows=load('io-records.json');status=load('io-status.json')
assert m['source_restored'] and not m['performance_qualified']
for f,digest in m['source_sha256'].items():assert h(subprocess.check_output(['git','show',m['source_commit']+':'+f]))==digest,f
assert h((p/'helper.rs').read_bytes())==m['helper_sha256'] and h((p/'io-oracle.rs').read_bytes())==m['oracle_sha256']
assert len(status)==12 and {r['repeat']for r in status}==set(range(1,13))and len({r['pid']for r in status})==12 and all(r['exit_code']==0 for r in status)
expected={(i,c,mode,sample,role)for i in range(1,13)for c in ['scalar','vector768','vector14000','vector220000']for mode in ['normal','full']for sample in range(12)for role in ['baseline','control','candidate']}
index={(r['repeat'],r['case'],r['mode'],r['sample'],r['role']):r for r in rows};assert len(rows)==len(index)==3456 and set(index)==expected
for i in range(1,13):
 text=(p/'io'/f'{i:02}.log').read_text();parsed=[dict(repeat=i,**json.JSONDecoder().raw_decode(s)[0])for s in text.split('SINGLE_BUFFER_IO ')[1:]];assert parsed==[r for r in rows if r['repeat']==i]
for r in rows:assert type(r['ns'])is int and r['ns']>0 and r['payload_bytes']>0 and r['frame_bytes']==r['payload_bytes']+13
med={}
for i,c,mode,sample,role in expected:
 key=i,c,mode,role
 if key not in med:med[key]=statistics.median(index[i,c,mode,j,role]['ns']for j in range(12))
results=[];accepted=True
for c in ['scalar','vector768','vector14000','vector220000']:
 for mode in ['normal','full']:
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    a,b,d=[med[i,c,mode,role]for role in ['baseline','control','candidate']];pairs.append(dict(repeat=i,baseline_ns=a,control_ns=b,candidate_ns=d,gain_percent=100*(1-d/a),aa_percent=100*(1-b/a)))
   gain=statistics.median(r['gain_percent']for r in pairs);aa=statistics.median(abs(r['aa_percent'])for r in pairs);positive=sum(r['gain_percent']>0 for r in pairs);target=c=='vector768';passed=(positive==6 and gain>aa)if target else gain>=-max(2,aa);accepted &=passed
   results.append(dict(case=c,mode=mode,series=label,target=target,gain_percent=gain,aa_absolute_percent=aa,positive_pairs=positive,passes=passed,pairs=pairs))
q=dict(component_gate_passes=accepted,full_engine_qualified=False,full_objective_achieved=False,observations=len(rows),results=results);(p/'io-summary.json').write_text(json.dumps(q,indent=2)+'\n')
for r in results:print(r['case'],r['mode'],r['series'],round(r['gain_percent'],3),round(r['aa_absolute_percent'],3),r['positive_pairs'],r['passes'])
print('Independent raw/source audit PASS; component gate:',accepted)
