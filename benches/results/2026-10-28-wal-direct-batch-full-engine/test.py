from pathlib import Path
import json,hashlib,subprocess,os
p=Path(__file__).resolve().parent;source=Path('src/persist.rs');before=source.read_bytes();h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),source_sha256={str(f):h(f)for f in Path('src').rglob('*.rs')},helper_sha256=h(p/'helper.rs'),oracle_sha256=h(p/'oracle.rs'),performance_qualified=False)
try:
 candidate=before.decode();needle='    rmp_serde::encode::write_named(buf, rec).map_err(io_err)?;';assert candidate.count(needle)==1;candidate=candidate.replace(needle,'    wal_direct_zero::encode(buf, rec)?;');candidate+='\n'+(p/'helper.rs').read_text();(p/'candidate-persist.rs').write_text(candidate);source.write_text(candidate+'\n'+(p/'oracle.rs').read_text())
 args=['cargo','test','--release','--lib'];full=bool(os.environ.get('LIN_ZERO_FULL_SUITE'));args+=['--tests']if full else['wal_direct_zero_'];m['full_suite']=full
 with(p/('full-tests.log'if full else'tests.log')).open('w')as log:r=subprocess.run(args+['--','--nocapture'],stdout=log,stderr=subprocess.STDOUT)
 m['exit_code']=r.returncode;assert r.returncode==0,'inspect tests.log'
finally:
 source.write_bytes(before);m['source_restored']=all(h(f)==v for f,v in m['source_sha256'].items());(p/'test-status.json').write_text(json.dumps(m,indent=2)+'\n')
