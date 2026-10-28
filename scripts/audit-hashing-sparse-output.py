#!/usr/bin/env python3
"""Audit frozen sparse hashing output component source and recompute every paired observation."""
from pathlib import Path
import json,subprocess,hashlib,statistics,math,itertools,argparse,re
ROOT=Path(__file__).resolve().parents[1]
HEAD='6f0b233455684c15a1613a06eee33157986bde03'
INPUT='benches/results/2026-10-28-hashing-sparse-output-component'
CASES=['compare/insert_bulk_10k/lin', 'compare/insert_bulk_1k/lin', 'compare/point_get/lin', 'compare/filter_eq/lin', 'compare/update_1row_1k/lin', 'compare/wal_scalar_full_update/lin', 'compare/wal_scalar_normal_update/lin', 'compare/insert_bulk_10k/sqlite', 'compare/filter_eq/duckdb']
def sha(data):return hashlib.sha256(data).hexdigest()
def git(name,ref=HEAD):return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
def eligible_host(host):
 assert type(host['rust_available_parallelism'])is int and host['rust_available_parallelism']>=1 and host['candidate_sparse_output']is True

SPARSE_COPY='        let out = if self.touched.len() * 5 < self.v.len() {\n            let mut output = Arc::<[f32]>::new_uninit_slice(self.v.len());\n            let slots = Arc::get_mut(&mut output).expect("new output is uniquely owned");\n            slots.fill(std::mem::MaybeUninit::new(0.0));\n            for &i in &self.touched {\n                slots[i as usize].write(self.v[i as usize]);\n            }\n            // SAFETY: every element was initialized to valid f32 zero above;\n            // touched elements were then overwritten with initialized f32 values.\n            unsafe { output.assume_init() }\n        } else {\n            Arc::from(&self.v[..])\n        };\n'
def sparse_only(original,candidate):
 needle='        let out = Arc::from(&self.v[..]);\n';assert original.count(needle)==1
 start=candidate.index('    #[test]\n    fn sparse_arc_output_preserves_dense_bits_thresholds_and_held_results()')
 end=candidate.index('    #[test]\n    fn bigram_table_matches_slice_hash_for_every_byte_pair()',start)
 candidate=candidate[:start]+candidate[end:]
 assert candidate==original.replace(needle,SPARSE_COPY),'unrelated production source change'

def resource(data):
 text=data.decode();assert re.search(r'Exit status: 0(?:\n|$)',text)
 result={}
 for label,key in [('User time (seconds)','user_seconds'),('System time (seconds)','system_seconds'),('Maximum resident set size (kbytes)','peak_rss_kib')]:
  match=re.search(re.escape(label)+r':\s*([0-9.]+)',text);assert match,label;value=float(match.group(1));assert math.isfinite(value)and value>=0;result[key]=value
 assert result['peak_rss_kib']>0
 return result

def matrix(raw):
 provenance=json.loads((raw/'provenance.json').read_text());records=json.loads((raw/'process-status.json').read_text());expected=set(itertools.product(range(1,13),['baseline','control','candidate'],CASES));assert len(records)==324
 assert {(r['round'],r['mode'],r['case'])for r in records}==expected
 medians={};resources={}
 for r in records:
  assert r['exit_code']==0 and math.isfinite(r['elapsed_seconds'])and r['elapsed_seconds']>0
  assert r['binary_sha256']==provenance['binaries']['baseline'if r['mode']=='control'else r['mode']]['sha256']
  data=(raw/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')).with_suffix('.resources.txt').read_bytes();assert sha(data)==r['resource_sha256'];resources[r['round'],r['mode'],r['case']]=resource(data)
  run=json.loads((raw/f"{r['round']:02}-{r['mode']}"/r['case'].replace('/','_')/'run.json').read_text());assert run['status']=='complete'and len(run['cases'])==1 and run['cases'][0]['id']==r['case']and len(run['observations'])==8
  values=[]
  for o in run['observations']:
   assert o['case']==r['case']and o['metric']=='wall'and o['operations']==1
   v=float(o['value']);assert math.isfinite(v)and v>0;values.append(v)
  medians[r['round'],r['mode'],r['case']]=statistics.median(values)
 summary={};passed=True
 for case in CASES:
  summary[case]={};target=case=='compare/insert_bulk_10k/lin'
  for label,start in [('series1',1),('series2',7)]:
   pairs=[]
   for i in range(start,start+6):
    b,a,c=[medians[i,mode,case]for mode in ['baseline','control','candidate']]
    pairs.append(dict(round=i,baseline_ns=b,control_ns=a,candidate_ns=c,gain_percent=100*(1-c/b),aa_percent=100*(1-a/b)))
   gain=statistics.median(p['gain_percent']for p in pairs);noise=statistics.median(abs(p['aa_percent'])for p in pairs);positive=sum(p['gain_percent']>0 for p in pairs)
   ok=(positive==6 and gain>noise)if target else gain>=-max(2,noise);passed&=ok
   summary[case][label]=dict(target=target,gain_percent=gain,aa_absolute_percent=noise,positive_pairs=positive,passes=ok,pairs=pairs)
 qualification=dict(measurement_complete=True,component_gate_passes=passed,full_engine_qualified=False,full_objective_achieved=False,processes=324,observations=2592)
 assert json.loads((raw/'qualification.json').read_text())==qualification
 assert json.loads((raw/'summary.json').read_text())==summary
 return dict(audit_complete=True,**qualification,failed_target_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if w['target']),failed_control_windows=sum(not w['passes']for rows in summary.values()for w in rows.values()if not w['target']),summary=summary,process_resource_summary={case:{mode:{key:statistics.median(resources[i,mode,case][key]for i in range(1,13))for key in ['user_seconds','system_seconds','peak_rss_kib']}for mode in ['baseline','control','candidate']}for case in CASES},resource_scope='whole fresh process including setup/validation/drop')
def audit(parent):
 raw=parent/'hashing-sparse-output-component';run=json.loads((parent/'run-metadata.json').read_text());assert run.get('headSha',run.get('head_sha'))==HEAD and run['status']=='completed'and run['conclusion']in ['success','failure']
 frozen=git(INPUT+'/frozen-inputs.json');assert(raw/'frozen-inputs.json').read_bytes()==frozen
 for name,h in json.loads(frozen).items():
  data=git(name);assert sha(data)==h,name
  if name.startswith(INPUT+'/'):assert(raw/name[len(INPUT)+1:]).read_bytes()==data,name
 manifest=json.loads((raw/'input.json').read_text());assert(raw/'baseline-embed.rs').read_bytes()==git('src/embed.rs',manifest['baseline_reference']);assert(raw/'candidate-embed.rs').read_bytes()==git('src/embed.rs',manifest['candidate_reference'])==git('src/embed.rs')
 for name,h in manifest['source_sha256'].items():
  assert sha(git(name))==h
  if name!='src/embed.rs':assert sha(git(name,manifest['baseline_reference']))==h,'unrelated baseline source change'
 tests=json.loads((raw/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==manifest['source_sha256']
 counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/'release-tests.log').read_text());assert[sum(int(c[i])for c in counts)for i in range(3)]==[450,0,2]
 prov=json.loads((raw/'provenance.json').read_text());assert prov['source_commit']==HEAD and prov['source_restored']and prov['instrumentation']is False and prov['source_sha256']==manifest['source_sha256']and prov['compare_sha256']==sha(git(INPUT+'/compare.rs'))
 for role in ['baseline','candidate']:assert prov['binaries'][role]['embed_sha256']==sha((raw/f'{role}-embed.rs').read_bytes())
 smokes=json.loads((raw/'smoke-status.json').read_text());assert len(smokes)==18 and {(s['role'],s['case'])for s in smokes}==set(itertools.product(['baseline','candidate'],CASES))and all(s['exit']==0 for s in smokes)
 for s in smokes:
  q=json.loads((raw/'smoke'/(s['role']+'-'+s['case'].replace('/','_'))/'run.json').read_text());assert q['status']=='complete'and len(q['cases'])==1 and q['cases'][0]['id']==s['case']and len(q['observations'])==1
 eligible_host(json.loads((raw/'host.json').read_text()))
 sparse_only((raw/'baseline-embed.rs').read_text(),(raw/'candidate-embed.rs').read_text())
 mutant=json.loads((raw/'mutant-status.json').read_text());assert mutant['runtime_mutant_rejected']and mutant['all_sources_restored']and mutant['exit']!=0 and mutant['original_source_sha256']==manifest['source_sha256']
 needle=b'            for &i in &self.touched {\n                slots[i as usize].write(self.v[i as usize]);\n            }';original=git('src/embed.rs');assert original.count(needle)==1;expected=original.replace(needle,b'            for &i in self.touched.iter().skip(1) {\n                slots[i as usize].write(self.v[i as usize]);\n            }');assert (raw/'mutant-embed.rs').read_bytes()==expected and sha(expected)==mutant['mutant_sha256']
 log=(raw/'mutant-runtime.log').read_text();assert 'test result: FAILED. 0 passed; 1 failed;'in log and 'panicked'in log
 assert 'FSTYPE'in(raw/'linux-environment.txt').read_text()
 result=matrix(raw);return dict(**result,run_head=HEAD,release_tests=450,ignored_tests=2,correctness_smokes=18,host_recorded=True,sparse_output_only=True,missing_coordinate_runtime_mutant_rejected=True)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('result_directory',type=Path);a=p.parse_args();result=audit(a.result_directory.resolve());(a.result_directory/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
