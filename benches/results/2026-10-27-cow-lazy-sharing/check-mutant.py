from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;frozen=p.parent/'2026-10-27-cow-broad-engine';names=['exec','store','check','index'];before={n:Path(f'src/{n}.rs').read_bytes()for n in names};needle='self.as_map() == other.as_map()';mutant=(p/'candidate-store.rs').read_text();assert mutant.count(needle)==1;mutant=mutant.replace(needle,'!matches!((&self.0, &other.0), (RowStorage::Inline(_), RowStorage::Shared(_)) | (RowStorage::Shared(_), RowStorage::Inline(_))) && self.as_map() == other.as_map()')
try:
 for n in names:Path(f'src/{n}.rs').write_bytes(((p if n in ['exec','store']else frozen)/f'candidate-{n}.rs').read_bytes())
 Path('src/store.rs').write_text(mutant+'\n'+(p/'oracle.rs').read_text())
 with(p/'mutant.log').open('w')as log:r=subprocess.run(['cargo','test','--lib','lazy_row_oracle::representation_preserves_eq_hash_wire_debug_and_snapshot_isolation','--color','never'],stdout=log,stderr=subprocess.STDOUT)
 log=(p/'mutant.log').read_text();assert r.returncode!=0 and 'test result: FAILED'in log and 'error[E'not in log
 (p/'mutant-status.json').write_text(json.dumps(dict(exit_code=r.returncode,expected_failure=True,mutant='cross-representation Eq falsely reports unequal',runtime_assertion_caught=True),indent=2)+'\n')
finally:
 for n,b in before.items():Path(f'src/{n}.rs').write_bytes(b)
