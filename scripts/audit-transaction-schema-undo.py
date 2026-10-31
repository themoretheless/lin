#!/usr/bin/env python3
"""Audit the frozen Linux terminal-checkpoint correction and exact baseline failures."""
from pathlib import Path
import argparse,functools,hashlib,json,re,subprocess
ROOT=Path(__file__).resolve().parents[1]
HEAD='18119ab943334a8b82d9753fe88b9ef40da4540b'
PREFIX='benches/results/2026-11-01-transaction-schema-undo-fix/'
PROBE='benches/results/2026-11-01-transaction-schema-undo-probe/'
CASES=[
 ('lost-prefix-journal','let prefix = std::mem::replace(&mut self.undo, Undo::None);','let _discarded = std::mem::replace(&mut self.undo, Undo::None); let prefix = Undo::None;',1,'generated_ids_are_reusable_after_schema_snapshot_rollback'),
 ('missing-checkpoint','self.checkpoint_undo();','();',2,'schema_after_rows_rollback_restores_rows_metadata_and_constraints'),
 ('reversed-checkpoint-order','self.store.mem_restore(snapshot);\n                self.rollback(prefix, None);','self.rollback(prefix, None);\n                self.store.mem_restore(snapshot);',1,'repeated_schema_changes_preserve_journal_order_and_abort_state'),
]
def sha(b):return hashlib.sha256(b).hexdigest()
@functools.lru_cache(maxsize=None)
def blob(name,ref=HEAD):return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
@functools.lru_cache(maxsize=None)
def source_map(ref,prefix):
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--',prefix],cwd=ROOT,text=True).splitlines()
 return {n:sha(blob(n,ref))for n in names if n.endswith('.rs')}
def audit(parent):
 parent=Path(parent);raw=parent/'transaction-schema-undo-correctness';run=json.loads((parent/'run-metadata.json').read_text())
 assert run['headSha']==HEAD and run['status']=='completed'and run['conclusion']=='success'
 frozen=json.loads(blob(PREFIX+'frozen-inputs.json'));assert(raw/'frozen-inputs.json').read_bytes()==blob(PREFIX+'frozen-inputs.json')
 for n,h in frozen.items():
  assert sha(blob(n))==h,n
  if n.startswith(PREFIX):assert sha((raw/n[len(PREFIX):]).read_bytes())==h,n
 m=json.loads(blob(PREFIX+'input.json'));base=m['baseline_reference'];assert base=='21dcd7d81354aa2f59038aa6cb142c86cd1cd1f5'
 assert m['source_sha256']==source_map(HEAD,'src')==source_map(m['candidate_reference'],'src')
 assert m['tests_sha256']==source_map(HEAD,'tests')
 original=source_map(base,'src');assert set(original)==set(m['source_sha256'])
 assert {n for n,h in original.items()if h!=m['source_sha256'][n]}=={'src/exec.rs'}
 for n in ['Cargo.toml','Cargo.lock']:assert blob(n)==blob(n,base)
 status=json.loads((raw/'qualification-status.json').read_text());assert status['source_commit']==HEAD and status['sources_restored']and status['tests_unchanged']and not status['performance_qualified']and status['test.py']==status['check-mutants.py']==0
 tests=json.loads((raw/'test-status.json').read_text());assert tests['exit_code']==0 and tests['source_sha256']==m['source_sha256']and tests['tests_sha256']==m['tests_sha256']and tests['release_passed']==458 and tests['release_ignored']==2 and not tests['performance_qualified']
 assert 'rustc 1.96.0 'in tests['compiler']and 'host: x86_64-unknown-linux-gnu'in tests['compiler']
 log=(raw/'full-release.log').read_text();counts=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)
 assert[sum(int(c[i])for c in counts)for i in range(3)]==[458,0,2]and 'test result: FAILED.'not in log
 for _,_,_,_,test in CASES:assert 'test '+test+' ... ok'in log,test
 mutants=json.loads((raw/'mutant-status.json').read_text());assert mutants['source_restored']and len(mutants['checks'])==3 and {c['label']for c in mutants['checks']}=={c[0]for c in CASES}
 text=blob('src/exec.rs').decode()
 for label,old,new,count,test in CASES:
  assert text.count(old)==count;expected=text.replace(old,new).encode();assert(raw/(label+'-mutant.rs')).read_bytes()==expected
  c=next(c for c in mutants['checks']if c['label']==label);assert c['exit_code']==101 and c['compiled_runtime_rejected']and c['test']==test and c['mutant_sha256']==sha(expected)
  log=(raw/(label+'-runtime.log')).read_text();assert 'Finished `release` profile'in log and 'running 1 test'in log and 'test '+test+' ... FAILED'in log and 'test result: FAILED. 0 passed; 1 failed;'in log
 probe=json.loads(blob(PROBE+'status.json'));assert probe['baseline_reference']==base and probe['source_sha256']==original and probe['source_restored']and probe['runtime_exit']==3 and probe['runtime_reproduced']and probe['probe_sha256']==sha(blob(PROBE+'probe.rs'))
 baseline_log=blob(PROBE+'baseline-runtime.log').decode();outcomes=[json.loads(s)for s in baseline_log.splitlines()if s.startswith('{')];assert outcomes==probe['outcomes']and len(outcomes)==6 and all(not o['passes']for o in outcomes)
 assert {(o['case'],o['explicit'])for o in outcomes}=={(name,explicit)for name in ['append_then_schema','update_then_schema','delete_then_unique_index']for explicit in [False,True]}
 assert 'Finished `release` profile'in baseline_log
 return dict(audit_complete=True,run_head=HEAD,release_passed=458,release_ignored=2,runtime_negatives_rejected=3,baseline_runtime_failures_verified=6,source_scope='only exec.rs differs from corrected baseline',qualification_scope='default library/integration tests and 27 exercised schema rollback scenarios',performance_qualified=False,full_objective_achieved=False)
if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);args=p.parse_args();result=audit(args.directory);(args.directory/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
