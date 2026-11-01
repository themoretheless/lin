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
assert provenance['original_harness_sha256']==frozen['files_sha256']['original-compare.rs']
assert provenance['instrumented_harness_sha256']==frozen['files_sha256']['instrumented-compare.rs']
for name in ['exec','persist','store','lib']:assert provenance['instrumented_source_sha256'][name]==frozen['files_sha256']['instrumented-'+name+'.rs']
status=json.loads((p/'test-status.json').read_text());assert status['exit_code']==0
counts=[tuple(map(int,x))for x in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',(p/'tests.log').read_text())];assert counts and sum(x[0]for x in counts)==458 and sum(x[1]for x in counts)==0 and sum(x[2]for x in counts)==2
smokes=json.loads((p/'smoke-status.json').read_text());assert len(smokes)==12 and len({(r['case'],r['role'])for r in smokes})==12 and all(r['exit_code']==0 for r in smokes)
# Reconstruct the declared process order and commands independently.
processes=json.loads((p/'process-status.json').read_text())
cases=[f'compare/durable_{op}_1row_{size}/lin' for size in ['1k','10k'] for op in ['update','rollback']]
expected=[]
for repeat in range(1,7):
 ordered=cases[repeat%4:]+cases[:repeat%4]
 for case_index,case in enumerate(ordered):
  roles=['original','control','disabled','traced'];shift=(repeat+case_index)%4;roles=roles[shift:]+roles[:shift]
  expected.extend((repeat,role,case) for role in roles)
assert [(r['round'],r['mode'],r['case']) for r in processes]==expected
for row in processes:
 role=row['mode'];binary=provenance['binaries']['original' if role in ['original','control'] else 'instrumented']
 assert row['binary_sha256']==binary['sha256'] and row['trace_enabled']==(role=='traced')
 cmd=row['command'];assert cmd[:5]==['/usr/bin/time','-v','-o',str(p/f"{row['round']:02}-{role}"/(row['case'].replace('/','_')+'-resource.txt')),'--']
 assert cmd[5:]==[binary['path'],'--filter',row['case'],'--samples','8','--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(p/f"{row['round']:02}-{role}"/row['case'].replace('/','_'))]
q=json.loads((p/'qualification.json').read_text());assert q['diagnostic_complete']and q['processes']==96 and q['observations']==768 and not q['source_candidate_qualified'] and not q['full_objective_achieved']
# Recompute from raw in an isolated copy and compare all generated evidence;
# never overwrite archived outputs or current project source.
with tempfile.TemporaryDirectory(prefix='lin-cow-phase-audit-')as td:
 target=Path(td)/'raw';shutil.copytree(p,target)
 result=subprocess.run(['python3',str(p/'analyze.py'),str(target)],capture_output=True,text=True);assert result.returncode==0,result.stderr
 for name in ['summary.json','qualification.json','trace-manifest.json','report.md']:assert (target/name).read_bytes()==(p/name).read_bytes(),name
(p/'raw-source-audit.json').write_text(json.dumps(dict(audit_complete=True,processes=96,observations=768,release_tests_passed=458,release_tests_ignored=2,smokes=12,source_restored=True,source_candidate_qualified=False,full_objective_achieved=False,scope='Frozen diagnostic raw/source audit; not independent timing qualification'),indent=2)+'\n');print('Frozen raw/source audit passed; diagnostic only')
