from pathlib import Path
import json,hashlib,re,subprocess,tempfile,shutil
p=Path(__file__).resolve().parent;h=lambda data:hashlib.sha256(data).hexdigest()
frozen=json.loads((p/'input-provenance.json').read_text());provenance=json.loads((p/'provenance.json').read_text())
for name,digest in frozen['files_sha256'].items():assert h((p/name).read_bytes())==digest,name
for section in ['source_sha256','build_inputs_sha256']:
 for name,digest in frozen[section].items():assert h(Path(name).read_bytes())==digest,name
assert provenance['source_restored']and provenance['source_sha256']==frozen['source_sha256']
assert provenance['original_compare_sha256']==frozen['build_inputs_sha256']['benches/compare.rs']
assert provenance['profile_sha256']==frozen['files_sha256']['profile.rs']
for name in ['exec','persist','store','lib']:assert provenance['instrumented_source_sha256'][name]==frozen['files_sha256']['instrumented-'+name+'.rs']
status=json.loads((p/'test-status.json').read_text());assert status['exit_code']==0
counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(p/'tests.log').read_text())];assert counts and sum(x[0]for x in counts)==445 and sum(x[1]for x in counts)==0 and sum(x[2]for x in counts)==2
smokes=json.loads((p/'smoke-status.json').read_text());assert len(smokes)==30 and len({(r['case'],r['role'])for r in smokes})==30 and all(r['exit_code']==0 for r in smokes)
q=json.loads((p/'qualification.json').read_text());assert q['diagnostic_complete']and q['processes']==240 and q['observations']==1920 and not q['source_candidate_qualified'] and not q['full_objective_achieved']
# Recompute from raw in an isolated copy and compare all generated evidence;
# never overwrite archived outputs or current project source.
with tempfile.TemporaryDirectory(prefix='lin-cow-phase-audit-')as td:
 target=Path(td)/'raw';shutil.copytree(p,target)
 result=subprocess.run(['python3',str(p/'analyze.py'),str(target)],capture_output=True,text=True);assert result.returncode==0,result.stderr
 for name in ['summary.json','qualification.json','trace-manifest.json','report.md']:assert (target/name).read_bytes()==(p/name).read_bytes(),name
(p/'raw-source-audit.json').write_text(json.dumps(dict(audit_complete=True,processes=240,observations=1920,release_tests_passed=445,release_tests_ignored=2,smokes=30,source_restored=True,source_candidate_qualified=False,full_objective_achieved=False,scope='Frozen diagnostic raw/source audit; not independent timing qualification'),indent=2)+'\n');print('Frozen raw/source audit passed; diagnostic only')
