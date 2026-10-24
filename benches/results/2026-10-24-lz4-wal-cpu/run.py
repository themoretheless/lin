from pathlib import Path
import subprocess,json,hashlib,random
p=Path(__file__).resolve().parent
binary='/tmp/lin-lz4-wal-cpu-gate'
m=json.loads((p/'provenance.json').read_text());assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==m['binary_sha256']
def crc32c(data):
 table=[]
 for v in range(256):
  for _ in range(8):v=(v>>1)^(0x82f63b78 if v&1 else 0)
  table.append(v)
 value=0xffffffff
 for b in data:value=(value>>8)^table[(value^b)&255]
 return value^0xffffffff
fixtures=[]
for n in [1000,10000]:
 path=p/f'fixture-{n}';assert not path.exists()
 with (p/f'emit-{n}.log').open('w') as log:r=subprocess.run([binary,'emit',str(path),str(n)],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 raw=(path/'log').read_bytes();offset=0;frames=[]
 while offset<len(raw):
  assert raw[offset:offset+4]==b'LIN\x06';size=int.from_bytes(raw[offset+4:offset+8],'little');frame=raw[offset+8:offset+8+size];assert len(frame)==size
  assert crc32c(frame[:-4])==int.from_bytes(frame[-4:],'little')
  frames.append({'codec':frame[0],'payload_bytes':len(frame)-5});offset+=8+size
 assert len(frames)==2 and frames[-1]['codec']==3
 target=p/f'payload-{n}.bin';target.write_bytes(frame[1:-4]);fixtures.append({'rows':n,'frames':frames,'wal_sha256':hashlib.sha256(raw).hexdigest(),'payload_sha256':hashlib.sha256(target.read_bytes()).hexdigest()})
(p/'fixtures.json').write_text(json.dumps(fixtures,indent=2))
entropy=p/'entropy.bin';entropy.write_bytes(random.Random(20261024).randbytes((p/'payload-1000.bin').stat().st_size))
status=[]
for i in range(1,7):
 for name in ['1000','10000','entropy']:
  payload=p/(f'payload-{name}.bin' if name!='entropy' else 'entropy.bin')
  with (p/f'process-{i}-{name}.jsonl').open('w') as out,(p/f'process-{i}-{name}.log').open('w') as log:r=subprocess.run([binary,'cpu',str(payload),str(i)],stdout=out,stderr=log)
  assert r.returncode==0
  docs=[json.loads(l) for l in (p/f'process-{i}-{name}.jsonl').read_text().splitlines()];assert len(docs)==3 and all(x['exact_roundtrip'] and len(x['raw_checksum_ns'])==len(x['compress_checksum_ns'])==24 for x in docs)
  status.append({'process':i,'fixture':name,'exit_code':r.returncode});(p/'status.json').write_text(json.dumps(status,indent=2))
 print(i,'complete',flush=True)
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in m['source_sha256'].items())
