from pathlib import Path
import subprocess,json,os,hashlib
p=Path(__file__).resolve().parent
old=json.loads((p/'old-reader-provenance.json').read_text());assert hashlib.sha256(Path('/tmp/lin-old-reader-before-dictionary').read_bytes()).hexdigest()==old['binary_sha256']
fixture=Path(os.environ.get('LIN_DICT_FIXTURE_OUT',str(p/'compat-fixture-final')));assert not fixture.exists()
status={}
steps=[('compatibility',['cargo','test','--lib','dictionary_reopen_fixture_and_old_reader_rejection','--','--nocapture'],dict(os.environ,LIN_DICTIONARY_FIXTURE=str(fixture),LIN_DICTIONARY_OLD_READER='/tmp/lin-old-reader-before-dictionary')),('reader-tests',['cargo','test','--lib','dictionary_wal_tests'],os.environ),('workspace',['cargo','test','--workspace'],os.environ)]
for name,cmd,env in steps:
 with (p/(name+'.log')).open('w') as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,env=env)
 status[name]=r.returncode;(p/'checks.json').write_text(json.dumps(status,indent=2));print(name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
 if name=='compatibility':
  markers=[json.loads(line.split('DICT_COMPAT ',1)[1]) for line in (p/'compatibility.log').read_text().splitlines() if 'DICT_COMPAT ' in line]
  assert len(markers)==1 and markers[0]['files_unchanged'] and markers[0]['old_reader_exit']==2
  data=(fixture/'log').read_bytes();pos=0;codecs=[]
  while pos<len(data):
   assert data[pos:pos+4]==b'LIN\x06';size=int.from_bytes(data[pos+4:pos+8],'little');assert size>=5 and pos+8+size<=len(data);codecs.append(data[pos+8]);pos+=8+size
  assert 5 in codecs
  (p/'compatibility.json').write_text(json.dumps({'old_reader':markers[0],'framed_codecs':codecs,'fixture_file_sha256':{str(f.relative_to(fixture)):hashlib.sha256(f.read_bytes()).hexdigest() for f in fixture.rglob('*') if f.is_file()}},indent=2))
print('all reader gates passed',flush=True)
