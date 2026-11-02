from pathlib import Path
import ast,hashlib,json,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();run='41401044023';head='451fd7a25a6dc1f0f712eaef841aa8ee4bdd561d'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','databaseId,headSha,status,conclusion,url,jobs'],cwd=root,text=True))
assert meta['status']=='completed'and meta['headSha']==head and meta['conclusion']in('success','failure'),'Exact upstream must finish; do not replace with older correctness'
steps={s['name']:s['conclusion']for j in meta['jobs']for s in j['steps']}
for name in ['Verify frozen source pair and harness operation boundaries','Full current-source default and feature suites and five compiled negatives','Independently audit current source tests and compiled negative sources']:assert steps[name]=='success',name
raw=p/'correctness-artifacts';assert not raw.exists(),'preserve prior attempt';raw.mkdir()
subprocess.run(['gh','run','download',run,'--name','cow-extent-full-engine','--dir',str(raw)],cwd=root,check=True)
prefix='benches/results/2026-11-02-cow-extent-full-engine/';frozen=json.loads((raw/'frozen.json').read_text());assert(raw/'frozen.json').read_bytes()==subprocess.check_output(['git','show',head+':'+prefix+'frozen.json'],cwd=root)
for name,h in frozen.items():
 assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
 if name.startswith(prefix):assert sha((raw/Path(name).relative_to(prefix)).read_bytes())==h,name
m=json.loads((raw/'source-inputs.json').read_text());runtime=json.loads((p/'runtime-inputs.json').read_text());assert m['candidate']['commit']==runtime['runtime_reference']
for name,h in m['runtime_and_tests_sha256'].items():assert sha((root/name).read_bytes())==h,name
correct=json.loads((raw/'candidate-correctness.json').read_text());assert correct['complete']and correct['sources_and_tests_restored']and correct['source_commit']==runtime['runtime_reference']and correct['compiled_negative_checks']==5 and correct['compiled_negatives_exit_code']==0
assert 'release: 1.96.0'in correct['compiler']and'host: x86_64-unknown-linux-gnu'in correct['compiler']
for mode,passed,ignored in [('default',463,2),('feature',483,4)]:
 log=(raw/(mode+'-tests.log')).read_bytes();counts=[tuple(map(int,c))for c in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.decode())];assert dict(passed=sum(c[0]for c in counts),failed=sum(c[1]for c in counts),ignored=sum(c[2]for c in counts))==dict(passed=passed,failed=0,ignored=ignored)==correct['suites'][mode]['counts'];assert sha(log)==correct['suites'][mode]['log_sha256']and correct['suites'][mode]['exit_code']==0
source=subprocess.check_output(['git','show',runtime['runtime_reference']+':src/exec.rs'],cwd=root,text=True);tree=ast.parse((raw/'check-mutants.py').read_text());cases=next(ast.literal_eval(n.value)for n in tree.body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='cases'for t in n.targets));negative=json.loads((raw/'mutant-status.json').read_text());assert len(cases)==len(negative['checks'])==5 and negative['source_restored']
for (label,old,new,count,target,test),check in zip(cases,negative['checks']):
 assert source.count(old)==count;expected=source.replace(old,new).encode();assert(raw/(label+'-mutant.rs')).read_bytes()==expected;assert check==dict(label=label,exit_code=101,test=test,mutant_sha256=sha(expected),compiled_runtime_rejected=True)
 log=(raw/(label+'-runtime.log')).read_text();assert 'Finished `release` profile'in log and 'running 1 test'in log and 'test result: FAILED. 0 passed; 1 failed;'in log and 'test '+test+' ... FAILED'in log
q=json.loads((raw/'qualification.json').read_text());assert q['measurement_complete']and q['processes']==2412 and q['observations']==19296
# A failed source performance gate remains binding for adoption; peer discovery cannot waive it.
(p/'correctness-prerequisite.json').write_text(json.dumps(dict(upstream=meta,exact_linux_correctness_verified=True,default_passed=463,feature_passed=483,compiled_negative_checks=5,upstream_source_gate_passed=q['source_candidate_qualified'],upstream_full_objective_achieved=False,adoption_qualified=False,artifact_sha256={str(f.relative_to(raw)):sha(f.read_bytes())for f in sorted(raw.rglob('*'))if f.is_file()}),indent=2)+'\n');print('Exact current-source Linux correctness verified; upstream source gate remains binding')
