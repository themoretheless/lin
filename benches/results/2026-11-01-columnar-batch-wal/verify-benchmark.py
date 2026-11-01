from pathlib import Path
import json,hashlib,re,math,runpy
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();read=lambda f:json.loads((p/f).read_text())
frozen=read('frozen-benchmark.json');assert all(sha(Path(f).read_bytes())==h for f,h in frozen.items())
log=(p/'full-release.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log);assert sum(int(x[0])for x in counts)==460 and sum(int(x[2])for x in counts)==2
m=read('benchmark-provenance.json');assert m['source_restored']and m['binaries']['candidate']['persist_sha256']==sha(Path('src/persist.rs').read_bytes())and m['harness_sha256']==sha((p/'benchmark-compare.rs').read_bytes())
assert all(sha(Path(f).read_bytes())==h for f,h in m['other_source_sha256'].items())
for b in m['binaries'].values():assert sha(Path(b['path']).read_bytes())==b['sha256']
negative=read('mutant-status.json');assert negative['source_restored']and negative['runtime_assertion_caught']and negative['exit_code']==101
candidate=(p/'candidate-persist.rs').read_bytes();mutant=(p/'lost-child-mutant.rs').read_bytes();assert mutant==candidate.replace(b'for pack in &packs[1..] {',b'for pack in &packs[1..1] {')and sha(mutant)==negative['mutant_sha256']and negative['source_before_sha256']==sha(candidate)
assert 'column_batch_preserves_order_bits_and_existing_wire_replay ... FAILED'in(p/'lost-child-runtime.log').read_text()
functional=read('functional/status.json');assert functional['complete']and len(functional['cases'])==32 and all(r['exit_code']==0 and r['full_values_reopen']for r in functional['cases'])
legacy=read('legacy-results/status.json');oracle=read('legacy-oracle-negative/status.json');assert legacy['complete']and len(legacy['cases'])==2 and oracle['runtime_negative_caught']and oracle['wal_unchanged']
assert all(r['legacy_full_values_verified']and r['embedding_f32_bits_verified']and r['duplicate_apply_noop']and r['crc_corruption_rejected']for r in legacy['cases'])
lm=read('legacy-provenance.json');assert lm['legacy_reference']=='04749994b9ca7a090476b95401f0f7166d8b870a'and lm['sources_verified']and sha(Path(lm['binary']).read_bytes())==lm['binary_sha256']
assert read('benchmark-smoke/status.json')['complete'];cases=read('benchmark-cases.json');rows=read('benchmark-measurement/process-status.json');expected=[]
for rnd in range(1,13):
 for i,c in enumerate(cases[rnd%24:]+cases[:rnd%24]):
  roles=['baseline','control','candidate'];s=(rnd+i)%3
  expected.extend((rnd,role,c['id'])for role in roles[s:]+roles[:s])
assert [(r['round'],r['role'],r['id'])for r in rows]==expected and len(rows)==864
medians={};ids=set()
def median(values):
 v=sorted(values);n=len(v);return v[n//2]if n%2 else(v[n//2-1]+v[n//2])/2
for folder,count in [('benchmark-smoke',48),('benchmark-measurement',864)]:
 rs=read(folder+'/process-status.json');assert len(rs)==count
 for r in rs:
  b=m['binaries']['baseline'if r['role']=='control'else r['role']];out=p/folder/r['output'];samples='1'if folder=='benchmark-smoke'else'8'
  assert r['exit_code']==0 and r['binary_sha256']==b['sha256']and r['command']==[b['path'],'--filter',r['id'],'--samples',samples,'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(out)]
  d=json.loads((out/'run.json').read_text());assert d['status']=='complete'and len(d['cases'])==1 and d['cases'][0]['id']==r['id'] and len(d['observations'])==int(samples)
  assert d['id']not in ids;ids.add(d['id']);values=[]
  for o in d['observations']:
   v=float(o['value']);assert o['case']==r['id']and o['metric']=='wall'and o['operations']==1 and o['availability']['state']=='available'and math.isfinite(v)and v>0;values.append(v)
  if folder=='benchmark-measurement':medians[r['round'],r['role'],r['id']]=median(values)
summary=read('benchmark-measurement/summary.json');passed=True
for c in cases:
 for series,start in [('series1',1),('series2',7)]:
  gains=[];aa=[];pairs=[]
  for rnd in range(start,start+6):
   a,b,v=[medians[rnd,role,c['id']]for role in ['baseline','control','candidate']];gains.append(100*(1-v/a));aa.append(abs(100*(1-b/a)));pairs.append(dict(round=rnd,baseline_ns=a,control_ns=b,candidate_ns=v,gain_percent=100*(1-v/a),aa_percent=100*(1-b/a)))
  g=median(gains);noise=median(aa);positive=sum(v>0 for v in gains);ok=(positive==6 and g>noise)if c['target']else g>=-max(2,noise);r=summary[c['id']][series]
  assert r['target']==c['target']and r['pairs']==pairs
  assert abs(r['gain_percent']-g)<1e-10 and abs(r['aa_absolute_percent']-noise)<1e-10 and r['positive_pairs']==positive and r['passes']==ok;passed &=ok
q=read('benchmark-measurement/qualification.json');assert q['processes']==864 and q['observations']==6912 and q['component_gate_passes']==passed and not q['full_objective_achieved']
(p/'benchmark-independent-audit.json').write_text(json.dumps(dict(complete=True,processes=864,observations=6912,component_gate_passes=passed,compiled_lost_child_caught=True,legacy_f32_bit_negative_caught=True,full_objective_achieved=False),indent=2)+'\n')
print('PASS frozen source, loss negative, old reader, 48 smokes, 864 commands / 6912 observations and numerical gates')
