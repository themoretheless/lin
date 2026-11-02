import gzip,json,pathlib,sys,hashlib
p=pathlib.Path(__file__).resolve().parent
code=(p/'audit.py').read_text()
# Reuse the exact input validation, ELF mapping, and decoder, without the full-matrix loop.
assert code.count('counts=collections.Counter();details=[]')==1
exec(compile(code.split('counts=collections.Counter();details=[]')[0],str(p/'audit.py'),'exec'))
classes=json.loads(gzip.decompress((p/'classifications.json.gz').read_bytes()))
names={r['name'] for r in classes if r['classification']=='other_byte_differences'}
out=[]
for r in rows:
 if r['name'] not in names:continue
 aa=r['baseline_address'];ba=r['candidate_address']
 ai=list(md.disasm(body('baseline',aa,r['baseline_size']),aa))
 bi=list(md.disasm(body('candidate',ba,r['candidate_size']),ba))
 assert len(ai)==len(bi)
 changes=[]
 for a,b in zip(ai,bi):
  an,ash,af,_=normalize(bytes(a.bytes),a.address);bn,bsh,bf,_=normalize(bytes(b.bytes),b.address)
  assert af and bf and ash==bsh
  if an!=bn:
   changes.append(dict(offset=a.address-aa,baseline=dict(mnemonic=a.mnemonic,operands=a.op_str,bytes=bytes(a.bytes).hex()),candidate=dict(mnemonic=b.mnemonic,operands=b.op_str,bytes=bytes(b.bytes).hex())))
 assert changes
 out.append(dict(name=r['name'],changes=changes))
assert len(out)==len(names)
(p/'remaining-instruction-differences.json').write_text(json.dumps(dict(audit_source_sha256=hashlib.sha256(code.encode()).hexdigest(),functions=out),indent=2)+'\n')
print(json.dumps(dict(functions=len(out),instructions=sum(len(r['changes']) for r in out))))
