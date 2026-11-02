import collections, gzip, hashlib, json, pathlib, struct, subprocess, sys
EXPECTED = {
 'baseline':'4d9f45815d533440ec39f96dc1842298ed65333248e208df2ce4438ac85da56e',
 'candidate':'696349d7c5da1464901275fdab894bf9836d1845f5b5d67ababb6fb4694600ce'}
p = pathlib.Path(sys.argv[1]); out = pathlib.Path(__file__).resolve().parent
records = {}; binaries = {}; segments = {}; duplicates = {}
for role, expected in EXPECTED.items():
 b = (p / f'lin-corrected-full-engine-{role}').read_bytes()
 assert hashlib.sha256(b).hexdigest() == expected
 assert b[:6] == b'\x7fELF\x02\x01' and struct.unpack_from('<H',b,18)[0] == 62
 phoff = struct.unpack_from('<Q',b,32)[0]; phsize, phnum = struct.unpack_from('<HH',b,54)
 loads = []
 for i in range(phnum):
  kind,flags,offset,addr,_,filesz,_,_ = struct.unpack_from('<IIQQQQQQ',b,phoff+i*phsize)
  if kind == 1 and flags & 1: loads.append((addr,addr+filesz,offset))
 text = subprocess.check_output(['objdump','--syms','--demangle',str(p/f'lin-corrected-full-engine-{role}')],text=True)
 names = collections.defaultdict(list)
 for line in text.splitlines():
  parts=line.split(None,5)
  if len(parts)==6 and parts[2]=='F' and parts[3]=='.text' and 'duckdb::' in parts[5]:
   names[parts[5]].append((int(parts[0],16),int(parts[4],16)))
 duplicates[role]={k:v for k,v in names.items() if len(v)>1}
 records[role]={k:v[0] for k,v in names.items() if len(v)==1 and v[0][1]>0}
 binaries[role]=b;segments[role]=loads

def body(role, addr, size):
 for low,high,offset in segments[role]:
  if low<=addr and addr+size<=high:
   return binaries[role][offset+addr-low:offset+addr-low+size]
 raise AssertionError((role,addr,size))
a,b=records['baseline'],records['candidate']; common=sorted(a.keys() & b.keys())
rows=[]
for name in common:
 aa,az=a[name];ba,bz=b[name];ab=body('baseline',aa,az);bb=body('candidate',ba,bz)
 rows.append(dict(name=name,baseline_address=aa,candidate_address=ba,baseline_size=az,candidate_size=bz,identical_bytes=ab==bb,baseline_sha256=hashlib.sha256(ab).hexdigest(),candidate_sha256=hashlib.sha256(bb).hexdigest()))
summary=dict(binary_sha256=EXPECTED,common_unambiguous_nonzero_functions=len(rows),identical_body_bytes=sum(r['identical_bytes'] for r in rows),different_body_bytes=sum(not r['identical_bytes'] for r in rows),different_sizes=sum(r['baseline_size']!=r['candidate_size'] for r in rows),duplicate_symbol_names={k:len(v) for k,v in duplicates.items()},causal_conclusion=False,performance_qualification=False)
payload = (json.dumps(rows,separators=(',',':'))+'\n').encode()
(out/'function-bodies.json.gz').write_bytes(gzip.compress(payload, mtime=0))
summary['function_records_uncompressed_sha256'] = hashlib.sha256(payload).hexdigest()
(out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary))
