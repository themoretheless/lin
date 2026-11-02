from pathlib import Path
import hashlib,json,os,re,subprocess
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();module='exec::rollback_row_ownership_tests::'
assert not(p/'qualification.json').exists()
log=(p/'focused-tests.log').read_bytes();assert b'2 passed; 0 failed;'in log
names=subprocess.check_output(['git','ls-files','--','src','tests','lin-derive/src','tools/peer-worker/src','Cargo.toml','Cargo.lock','lin-derive/Cargo.toml','tools/peer-worker/Cargo.toml','examples/peer_bench.rs','.cargo','build.rs','rust-toolchain','rust-toolchain.toml'],cwd=root,text=True).splitlines()
original={n:(root/n).read_bytes()for n in names};before={n:sha(b)for n,b in original.items()};env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR='/tmp/lin-rollback-fts-delta-target-20261102')
assert not os.environ.get('RUSTFLAGS')and not os.environ.get('CARGO_ENCODED_RUSTFLAGS')and not any(k.startswith('CARGO_PROFILE_')for k in os.environ)
exec_source=original['src/exec.rs'].decode();store_source=original['src/store.rs'].decode()
# Keep both unchanged full legacy scalar/FTS restoration calls in every mutant.
assert 'self.index_remove_row(collection, idx, &current);'in store_source and 'self.fts_remove_row(collection, idx, &current);'in store_source
assert 'self.fts_insert_row(collection, idx, &current);'in store_source
mutations=[]
a=exec_source.index('    pub fn rollback(mut self)');b=exec_source.index('/// Row / edge / log byte',a);part=exec_source[a:b];assert part.count('self.packs.clear();')==2
part=part.replace('        self.packs.clear();\n','').replace('            self.packs.clear();\n','')
needle='self.db.store.r#gen = self.base_generation;';assert part.count(needle)==2;part=part.replace(needle,needle+' self.packs.clear();')
mutations.append(('retain-unpublished-owners','src/exec.rs',exec_source[:a]+part+exec_source[b:],module+'rollback_reuses_unheld_map_and_preserves_held_versions'))
needle='let mut current = std::mem::take(slot);';assert store_source.count(needle)==1
mutations.append(('clone-live-row','src/store.rs',store_source.replace(needle,'let mut current = slot.clone();'),module+'rollback_reuses_unheld_map_and_preserves_held_versions'))
a=store_source.index('    pub(crate) fn restore_row_delta(');b=store_source.index('\n    pub fn restore_rows(',a);part=store_source[a:b];install='        self.collections.get_mut(collection).unwrap()[idx] = current.clone();\n';assert part.count(install)==1
part=part.replace(install,'');anchor='        self.row_maps_sync_updated(collection, idx, identity_before.as_ref().unwrap_or(&current), &current);\n';assert part.count(anchor)==1;part=part.replace(anchor,anchor+install)
mutations.append(('rebuild-before-install','src/store.rs',store_source[:a]+part+store_source[b:],module+'owned_delta_installs_row_before_identity_and_empty_map_rebuild'))
negative=[]
try:
 for label,name,text,test in mutations:
  for n in ['src/exec.rs','src/store.rs']:(root/n).write_bytes(original[n])
  (root/name).write_text(text);(p/(label+'.rs')).write_text(text)
  command=['cargo','test','--release','--lib','rollback_row_ownership_tests'];log=p/(label+'.log');assert not log.exists()
  with log.open('w')as f:r=subprocess.run(command,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
  data=log.read_bytes();assert r.returncode!=0 and ('test '+test+' ... FAILED').encode()in data and b'1 passed; 1 failed;'in data,label
  negative.append(dict(label=label,changed_source=name,source_sha256=sha(text.encode()),command=command,exit=r.returncode,named_test_failed=test,log_sha256=sha(data)))
finally:
 for n in ['src/exec.rs','src/store.rs']:(root/n).write_bytes(original[n])
 assert before=={n:sha((root/n).read_bytes())for n in names}
suites=[]
for mode,passed,ignored in [('default',465,2),('feature',485,4)]:
 command=['cargo','test','--release']+(['--features','experimental-extent-wal']if mode=='feature'else[]);log=p/(mode+'-tests.log');assert not log.exists()
 with log.open('w')as f:r=subprocess.run(command,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
 data=log.read_bytes();assert r.returncode==0
 m=re.findall(rb'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',data);assert [sum(int(x[i])for x in m)for i in range(3)]==[passed,0,ignored]
 for test in ['rollback_reuses_unheld_map_and_preserves_held_versions','owned_delta_installs_row_before_identity_and_empty_map_rebuild']:assert ('test '+module+test+' ... ok').encode()in data
 suites.append(dict(mode=mode,command=command,passed=passed,failed=0,ignored=ignored,log_sha256=sha(data)))
assert before=={n:sha((root/n).read_bytes())for n in names}
(p/'qualification.json').write_text(json.dumps(dict(suites=suites,compiled_negatives=negative,source_sha256=before,source_restored=True,compiler=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False,external_peers=0),indent=2)+'\n');print('Full default465/2 and feature485/4 passed; three compiled negatives failed named assertions; exact source restored')
