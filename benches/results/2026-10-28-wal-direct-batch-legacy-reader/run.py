from pathlib import Path
import json,hashlib,subprocess
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored']and all(h(f)==v for f,v in m['source_sha256'].items());fixture=p/'fixtures';assert not fixture.exists();status={}
for role,mode in [('producer','produce'),('legacy','verify')]:
 binary=m['binaries'][role];assert h(binary['path'])==binary['sha256']
 with(p/(role+'.json')).open('w')as stdout,(p/(role+'.log')).open('w')as stderr:r=subprocess.run([binary['path'],mode,str(fixture)],stdout=stdout,stderr=stderr)
 status[role]=dict(exit_code=r.returncode,binary_sha256=h(binary['path']));(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');assert r.returncode==0,(role,'inspect log')
 assert h(binary['path'])==binary['sha256']and all(h(f)==v for f,v in m['source_sha256'].items())
reader=json.loads((p/'legacy.json').read_text());assert reader['legacy_reader_passed']and reader['full_canonical_rows']and reader['ieee_edge_record'];assert len(reader['outcomes'])==5
print(json.dumps(reader,indent=2))
