from pathlib import Path
import subprocess,json,hashlib,re
p=Path(__file__).resolve().parent;root=p.parents[2];raw=p/'update-patch-clone-elision-component';ref='f38e3178c003a234acbad6b8baff44d316407916';prefix='benches/results/2026-11-01-update-patch-clone-elision/'
git=lambda n:subprocess.check_output(['git','show',ref+':'+n],cwd=root)
h=lambda b:hashlib.sha256(b).hexdigest()
run=json.loads((p/'run-metadata.json').read_text());assert run['headSha']==ref and run['status']=='completed'and run['conclusion']=='failure'
failed=[s['name']for j in run['jobs']for s in j['steps']if s['conclusion']=='failure'];assert failed==['Build ordinary paired binaries']
freeze=json.loads(git(prefix+'frozen-inputs.json'));assert(raw/'frozen-inputs.json').read_bytes()==git(prefix+'frozen-inputs.json')
for n,d in freeze.items():
 assert h(git(n))==d,n
 if n.startswith(prefix):assert h((raw/n[len(prefix):]).read_bytes())==d,n
m=json.loads((raw/'input.json').read_text());tests=json.loads((raw/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==m['source_sha256']and tests['release_passed']==455 and tests['release_ignored']==2
counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(raw/'release-tests.log').read_text());assert[sum(int(c[i])for c in counts)for i in range(3)]==[455,0,2]
mut=json.loads((raw/'mutant-status.json').read_text());assert mut['source_restored']and len(mut['checks'])==2
for label,new in [('guard-derived-hash','if false {'),('skip-explicit-hash-guard','if k == "hash" {')]:
 expected=git('src/exec.rs').replace(b'if k == "hash" && !explicit_hash {',new.encode());assert(raw/(label+'-mutant.rs')).read_bytes()==expected
 c=next(c for c in mut['checks']if c['label']==label);assert c['exit_code']==101 and c['compiled_runtime_rejected']and c['mutant_sha256']==h(expected)
 log=(raw/(label+'-runtime.log')).read_text();assert 'Finished `release` profile'in log and 'test update_patch_preserves_explicit_and_derived_hash_guards ... FAILED'in log and 'test result: FAILED. 0 passed; 1 failed;'in log
prov=json.loads((raw/'provenance.json').read_text());assert prov['source_commit']==ref and prov['source_sha256']==m['source_sha256']and prov['source_restored']and prov['binaries']=={}
for n in ['process-status.json','summary.json','qualification.json','qualification-smoke-status.json']:assert not(raw/n).exists(),n
assert '--depth=1'in(p/'job.log').read_text()
assert "not in '21dcd7d81354aa2f59038aa6cb142c86cd1cd1f5'"in(p/'failed-step.log').read_text()
assert subprocess.check_output(['git','show',m['baseline_reference']+':src/exec.rs'],cwd=root)
result=dict(audit_complete=True,run_id=41249170021,run_head=ref,infrastructure_failure=True,release_passed=455,release_ignored=2,runtime_mutants_rejected=2,source_restored=True,timing_processes=0,timing_observations=0,component_qualified=False,cause='shallow depth=1 checkout omitted baseline source history')
(p/'functional-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
