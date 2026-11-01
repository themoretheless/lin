from pathlib import Path
import json,hashlib,subprocess,shutil,struct
p=Path(__file__).resolve().parent;m=json.loads((p/'legacy-provenance.json').read_text());assert hashlib.sha256(Path(m['binary']).read_bytes()).hexdigest()==m['binary_sha256'];assert json.loads((p/'legacy-results/status.json').read_text())['complete']
root=p/'legacy-oracle-negative';root.mkdir(exist_ok=False)
for f in (p/'fixture-1000-grouped').iterdir():shutil.copy2(f,root/f.name)
f=root/'expected.json';data=json.loads(f.read_text());row=data[0];values=row['embedding']['v'];i=next(i for i,v in enumerate(values)if abs(v)>1e-5);original=struct.unpack('>I',struct.pack('>f',values[i]))[0];changed=original^1;values[i]=struct.unpack('>f',struct.pack('>I',changed))[0];f.write_text(json.dumps(data,separators=(',',':')))
r=subprocess.run([m['binary'],str(root)],capture_output=True,text=True);(root/'stdout').write_text(r.stdout);(root/'stderr').write_text(r.stderr);assert r.returncode==101 and 'legacy embedding f32 bits'in r.stderr
(root/'status.json').write_text(json.dumps(dict(complete=True,runtime_negative_caught=True,exit_code=r.returncode,changed_row_id=row['id'],changed_dimension=i,original_f32_bits=original,mutated_f32_bits=changed,wal_unchanged=hashlib.sha256((root/'log.wal').read_bytes()).hexdigest()==hashlib.sha256((p/'fixture-1000-grouped/log.wal').read_bytes()).hexdigest(),performance_qualification=False),indent=2)+'\n');print('PASS actual old reader rejects one-bit f32 oracle corruption')
