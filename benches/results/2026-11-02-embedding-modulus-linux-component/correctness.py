from pathlib import Path
import hashlib,json,os,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];inputs=json.loads((p/'input.json').read_text());sha=lambda b:hashlib.sha256(b).hexdigest()
assert not(p/'correctness.json').exists()
for n,h in inputs['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
out=[]
for mode,passed,ignored in [('default',463,2),('feature',483,4)]:
 command=['cargo','test','--release']+(['--features','experimental-extent-wal']if mode=='feature'else[])
 log=p/(mode+'-full-tests.log');assert not log.exists()
 with log.open('w')as f:r=subprocess.run(command,cwd=root,env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(root/'target/embedding-modulus-cand')),stdout=f,stderr=subprocess.STDOUT)
 assert r.returncode==0
 b=log.read_bytes();m=re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',b)
 assert [sum(int(x[i])for x in m)for i in range(3)]==[passed,0,ignored]
 assert b'embed::tests::hashing_embedding_matches_original_dense_modulo_bits ... ok'in b
 out.append(dict(mode=mode,command=command,passed=passed,failed=0,ignored=ignored,log_sha256=sha(b)))
for n,h in inputs['files_sha256'].items():assert sha((root/n).read_bytes())==h,n
(p/'correctness.json').write_text(json.dumps(dict(suites=out,source_unchanged=True,performance_qualified=False),indent=2)+'\n')
