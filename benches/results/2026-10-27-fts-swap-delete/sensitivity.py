from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;names=['fts','store'];assert all(Path(f'src/{n}.rs').read_bytes()==(p/f'before-{n}.rs').read_bytes()for n in names);s=(p/'candidate-fts.rs').read_text();old='std::cmp::Ordering::Equal => { self.note_del(&new[j], from); i += 1; j += 1; }';assert s.count(old)==1;s=s.replace(old,'std::cmp::Ordering::Equal => { self.note_del(&new[j], to); self.note_del(&new[j], from); i += 1; j += 1; }');status={}
try:
 Path('src/fts.rs').write_text(s+'\n'+(p/'oracle.rs').read_text());Path('src/store.rs').write_bytes((p/'candidate-store.rs').read_bytes())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','fused_swap_delete_oracle'],stdout=log,stderr=subprocess.STDOUT)
 status['exit_code']=r.returncode
finally:
 for n in names:Path(f'src/{n}.rs').write_bytes((p/f'before-{n}.rs').read_bytes())
 (p/'sensitivity-status.json').write_text(json.dumps(status,indent=2)+'\n')
assert status['exit_code']==101 and 'pending_edits_and_large_token_fallback_match_rebuild ... FAILED' in (p/'mutant.log').read_text()
print('oracle rejects lost common destination posting; source restored',flush=True)
