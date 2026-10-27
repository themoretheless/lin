from pathlib import Path
import tempfile,shutil,json,subprocess,hashlib
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());reader=m['binaries']['legacy'];assert hashlib.sha256(Path(reader['path']).read_bytes()).hexdigest()==reader['sha256'];results={}
with tempfile.TemporaryDirectory(prefix='lin-zero-legacy-controls-')as tmp:
 for kind in ['frame_corruption','expected_rows_changed']:
  target=Path(tmp)/kind;shutil.copytree(p/'fixtures',target)
  f=target/('2.wal'if kind=='frame_corruption'else'2.rows');data=bytearray(f.read_bytes());data[11 if kind=='frame_corruption'else -1]^=1;f.write_bytes(data)
  r=subprocess.run([reader['path'],'verify',str(target)],capture_output=True,text=True)
  (p/(kind+'.log')).write_text(r.stdout+r.stderr)
  assert r.returncode!=0 and 'panicked at'in r.stderr
  if kind=='frame_corruption':assert 'checksum'in r.stderr or 'corrupt'in r.stderr
  else:assert 'assertion'in r.stderr
  results[kind]={'exit_code':r.returncode,'runtime_control_caught':True}
assert hashlib.sha256(Path(reader['path']).read_bytes()).hexdigest()==reader['sha256'];(p/'controls.json').write_text(json.dumps(results,indent=2)+'\n');print('PASS: separate legacy reader rejects corrupt frame and altered complete-row expectation at runtime')
