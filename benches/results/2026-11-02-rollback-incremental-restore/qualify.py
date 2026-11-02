from pathlib import Path
import hashlib,json,os,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest()
source=root/'src/store.rs';original=source.read_bytes();text=original.decode();name='incremental_restore_matches_legacy_after_folded_fts_and_partial_indexes'
assert not(p/'qualification.json').exists()
assert f'test store::borrowed_delete_tests::{name} ... ok'in(p/'focused-tests.log').read_text()
names=subprocess.check_output(['git','ls-files','--','src','tests','lin-derive/src','tools/peer-worker/src','Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','tools/peer-worker/Cargo.toml','examples/peer_bench.rs','.cargo','build.rs','rust-toolchain','rust-toolchain.toml'],cwd=root,text=True).splitlines()
before={n:sha((root/n).read_bytes())for n in names};env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR='/tmp/lin-rollback-fts-delta-target-20261102')
assert not os.environ.get('RUSTFLAGS') and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')
start=text.index('    pub(crate) fn restore_row_owned(');end=text.index('\n    pub fn restore_rows(',start);body=text[start:end]
mutations=[('omit-index-restore','        let _ = self.index_insert_row(collection, idx, &restored);','        // Mutant: index restoration omitted.'),('omit-fts-restore','        if let Some(fts) = self.fts.get_mut(collection) {\n            fts.sync_row(idx, &current, &restored);\n        }','        // Mutant: FTS restoration omitted.')]
negatives=[]
try:
 for label,old,new in mutations:
  assert body.count(old)==1
  mutant=text[:start]+body.replace(old,new)+text[end:];source.write_text(mutant);(p/(label+'.rs')).write_text(mutant)
  command=['cargo','test','--release','--lib',name];log=p/(label+'.log');assert not log.exists()
  with log.open('w')as f:r=subprocess.run(command,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
  b=log.read_bytes();assert r.returncode!=0 and f'test store::borrowed_delete_tests::{name} ... FAILED'.encode()in b and b'0 passed; 1 failed;'in b,label
  negatives.append(dict(label=label,command=command,exit=r.returncode,source_sha256=sha(mutant.encode()),log_sha256=sha(b),named_runtime_assertion_failed=True))
finally:
 source.write_bytes(original)
 assert before=={n:sha((root/n).read_bytes())for n in names}
suites=[]
for mode,passed,ignored in [('default',464,2),('feature',484,4)]:
 command=['cargo','test','--release']+(['--features','experimental-extent-wal']if mode=='feature'else[]);log=p/(mode+'-tests.log');assert not log.exists()
 with log.open('w')as f:r=subprocess.run(command,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 b=log.read_bytes();assert r.returncode==0
 m=re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',b)
 assert [sum(int(x[i])for x in m)for i in range(3)]==[passed,0,ignored]
 assert f'test store::borrowed_delete_tests::{name} ... ok'.encode()in b
 suites.append(dict(mode=mode,command=command,passed=passed,failed=0,ignored=ignored,log_sha256=sha(b)))
assert before=={n:sha((root/n).read_bytes())for n in names}
(p/'qualification.json').write_text(json.dumps(dict(suites=suites,compiled_negatives=negatives,source_sha256=before,source_restored=True,compiler=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False,external_peers=0),indent=2)+'\n')
print('Full default464/2 and feature484/4 passed; two compiled restoration mutants failed named assertions; exact source restored')
