from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest();m=json.loads((p/'legacy-provenance.json').read_text());assert m['sources_verified']and m['legacy_reference']=='04749994b9ca7a090476b95401f0f7166d8b870a'
assert sha(Path(m['binary']).read_bytes())==m['binary_sha256']and sha((p/'legacy-reader.rs').read_bytes())==m['driver_sha256']
snapshot=Path(m['snapshot'])
for f,h in m['source_sha256'].items():assert sha((snapshot/f).read_bytes())==h and sha(subprocess.check_output(['git','show',m['legacy_reference']+':'+f]))==h
fixtures=json.loads((p/'fixture-status.json').read_text());assert fixtures['complete'];root=p/'legacy-results';root.mkdir(exist_ok=False);results=[]
for fixture in fixtures['cases']:
 n=fixture['rows'];directory=p/f'fixture-{n}-grouped'
 for f,info in fixture['files'].items():
  b=(directory/f).read_bytes();assert len(b)==info['bytes']and sha(b)==info['sha256']
 cmd=[m['binary'],str(directory)];r=subprocess.run(cmd,capture_output=True,text=True);(root/f'{n}.stdout').write_text(r.stdout);(root/f'{n}.stderr').write_text(r.stderr);assert r.returncode==0,r.stderr[-2000:]
 data=json.loads(r.stdout);assert data['legacy_full_values_verified']and data['rows']==n and data['duplicate_apply_noop']and data['crc_corruption_rejected']and data['generation_verified']==int((directory/'expected-generation.txt').read_text())and not data['performance_qualified']
 results.append(dict(command=cmd,exit_code=r.returncode,**data))
(root/'status.json').write_text(json.dumps(dict(complete=True,legacy_reference=m['legacy_reference'],cases=results,full_objective_achieved=False,performance_qualification=False),indent=2)+'\n');print('PASS actual production reader: all 1000 and 10000 rows, generation, duplicate apply and CRC')
