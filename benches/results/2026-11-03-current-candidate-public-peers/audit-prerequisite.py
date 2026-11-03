from pathlib import Path
import subprocess,json,hashlib,math,statistics as st
assets=Path(__file__).resolve().parent;inputs=json.loads((assets/'input.json').read_text());run=inputs['source_gate_run'];dest=Path('/tmp/lin-current-public-peer-prerequisite-20261103');dest.mkdir(exist_ok=False)
meta=json.loads(subprocess.check_output(['gh','run','view',str(run),'--json','status,conclusion,headSha,jobs'],text=True));assert meta['status']=='completed' and meta['headSha']==inputs['runtime_reference'];assert meta['conclusion'] in ['success','failure']
job=meta['jobs'][0];assert next(s for s in job['steps'] if s['name'].startswith('Complete default workspace'))['conclusion']=='success'
subprocess.run(['gh','run','download',str(run),'-n','current-main-aggregate-discovery','-D',str(dest)],check=True)
provs=list(dest.rglob('build-provenance.json'));assert len(provs)==1;p=provs[0].parent;m=json.loads(provs[0].read_text());assert m['refs']['candidate']==inputs['runtime_reference']
for name,digest in m['builds']['candidate']['source_sha256'].items():assert hashlib.sha256(Path(name).read_bytes()).hexdigest()==digest
raw=p/'measurement';q=json.loads((raw/'decision.json').read_text());assert q['complete'] and (q['processes'],q['case_observations'],q['wall_samples'])==(144,1008,21888)
records=json.loads((raw/'records.json').read_text());plan=json.loads((raw/'plan.json').read_text());cases=plan['targets']+plan['controls'];assert len(records)==1008 and {(r['round'],r['rows'],r['role'],r['case'])for r in records}=={(i,n,role,c)for i in range(1,13)for n in [1000,10000,100000]for role in plan['roles']for c in cases}
med={};count=0
for r in records:
 data=json.loads((raw/r['file']).read_text());values=data['response']['samples_ns'];assert len(values)==(8 if r['case']=='insert_native'else 24) and all(math.isfinite(v)and v>0 for v in values);value=st.median(values);assert value==r['median_ns'];med[r['rows'],r['case'],r['round'],r['role']]=value;count+=len(values)
assert count==21888;summary=json.loads((raw/'summary.json').read_text());assert len(summary)==42
for s in summary:
 n,c=s['rows'],s['case'];start=1 if s['series']==1 else 7
 gains=[100*(1-med[n,c,i,'candidate']/med[n,c,i,'baseline'])for i in range(start,start+6)];aa=[abs(100*(1-med[n,c,i,'control']/med[n,c,i,'baseline']))for i in range(start,start+6)];g,a=st.median(gains),st.median(aa);positive=sum(v>0 for v in gains);ok=(positive==6 and g>a)if c in plan['targets']else g>=-max(2,a);assert (g,a,positive,ok)==(s['gain_percent'],s['aa_absolute_percent'],s['positive_pairs'],s['passes'])
assert q['component_gate_passes']==all(s['passes']for s in summary)
# A failed source numerical gate remains binding. Peer discovery does not waive it.
(dest/'audited-prerequisite.json').write_text(json.dumps({'source_gate_run':run,'runtime_reference':inputs['runtime_reference'],'default_correctness_passed':True,'complete_matrix_verified':True,'source_numeric_gate_passes':q['component_gate_passes'],'adoption_allowed':False},indent=2)+'\n');print('Upstream correctness and complete source matrix independently verified; any numerical failures remain binding')

subprocess.run([__import__('sys').executable,'benches/results/audit-aggregate-matrix-20261103.py','--raw',str(raw),'--output',str(dest/'independent-values-audit')],check=True)
