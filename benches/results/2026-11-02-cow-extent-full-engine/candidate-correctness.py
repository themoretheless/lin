from pathlib import Path
import hashlib,json,os,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'source-inputs.json').read_text());info=m['candidate'];status=dict(source_commit=info['commit'],performance_qualified=False,compiler=subprocess.check_output(['rustc','-Vv'],text=True),suites={})
assert not(p/'candidate-correctness.json').exists(),'preserve prior attempt'
source={str(f.relative_to(root)):sha(f.read_bytes())for folder in ['src','tests']for f in(root/folder).rglob('*')if f.is_file()};assert {n:h for n,h in source.items()if n.endswith('.rs')and n.startswith('src/')}==info['source_sha256']
try:
 for name,args,passed,ignored in [('default',[],463,2),('feature',['--features','experimental-extent-wal'],483,4)]:
  cmd=['cargo','test','--release',*args];log=p/(name+'-tests.log')
  with log.open('w')as output:r=subprocess.run(cmd,cwd=root,stdout=output,stderr=subprocess.STDOUT)
  counts=[tuple(map(int,c))for c in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log.read_text())];actual=dict(passed=sum(c[0]for c in counts),failed=sum(c[1]for c in counts),ignored=sum(c[2]for c in counts))
  status['suites'][name]=dict(command=cmd,exit_code=r.returncode,counts=actual,log_sha256=sha(log.read_bytes()));assert r.returncode==0 and actual==dict(passed=passed,failed=0,ignored=ignored)
 r=subprocess.run(['python3',str(p/'check-mutants.py')],cwd=root);status['compiled_negatives_exit_code']=r.returncode;assert r.returncode==0
 negative=json.loads((p/'mutant-status.json').read_text());assert len(negative['checks'])==5 and negative['source_restored']and all(c['compiled_runtime_rejected']for c in negative['checks']);status['compiled_negative_checks']=5;status['complete']=True
finally:
 status['sources_and_tests_restored']=source=={str(f.relative_to(root)):sha(f.read_bytes())for folder in ['src','tests']for f in(root/folder).rglob('*')if f.is_file()};(p/'candidate-correctness.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['sources_and_tests_restored']
