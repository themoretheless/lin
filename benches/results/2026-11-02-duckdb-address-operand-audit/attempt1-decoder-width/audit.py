"""Classify byte differences in exact DuckDB functions; not semantic equivalence."""
import gzip,hashlib,json,pathlib,struct,sys,collections
import capstone
from capstone.x86 import X86_OP_MEM,X86_REG_RIP
p=pathlib.Path(__file__).resolve().parent
source=p.parent/'2026-11-02-duckdb-elf-body-audit'
summary=json.loads((source/'summary.json').read_text())
payload=gzip.decompress((source/'function-bodies.json.gz').read_bytes())
assert hashlib.sha256(payload).hexdigest()==summary['function_records_uncompressed_sha256']
rows=json.loads(payload); binary_dir=pathlib.Path(sys.argv[1]); binaries={};segments={}
for role,expected in summary['binary_sha256'].items():
 b=(binary_dir/f'lin-corrected-full-engine-{role}').read_bytes();assert hashlib.sha256(b).hexdigest()==expected
 off=struct.unpack_from('<Q',b,32)[0];size,num=struct.unpack_from('<HH',b,54);loads=[]
 for i in range(num):
  kind,flags,offset,addr,_,filesz,_,_=struct.unpack_from('<IIQQQQQQ',b,off+i*size)
  if kind==1 and flags&1:loads.append((addr,addr+filesz,offset))
 binaries[role]=b;segments[role]=loads

def body(role,addr,size):
 for low,high,off in segments[role]:
  if low<=addr and addr+size<=high:return binaries[role][off+addr-low:off+addr-low+size]
 raise AssertionError((role,addr,size))
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64);md.detail=True

def normalize(data,addr):
 result=bytearray(data);decoded=0;shape=[];masked=0
 for ins in md.disasm(data,addr):
  assert ins.address==addr+decoded
  shape.append((ins.id,ins.size));start=decoded;decoded+=ins.size
  fields=[]
  if (ins.group(capstone.CS_GRP_CALL) or ins.group(capstone.CS_GRP_JUMP)) and ins.imm_size:
   fields.append((ins.imm_offset,ins.imm_size))
  if any(op.type==X86_OP_MEM and op.mem.base==X86_REG_RIP for op in ins.operands) and ins.disp_size:
   fields.append((ins.disp_offset,ins.disp_size))
  for off,size in fields:
   assert 0<=off and off+size<=ins.size
   result[start+off:start+off+size]=b'\0'*size;masked+=size
 return bytes(result),shape,decoded==len(data),masked
counts=collections.Counter();details=[]
for r in rows:
 if r['identical_bytes']:counts['raw_identical']+=1;continue
 aa=r['baseline_address'];ba=r['candidate_address'];az=r['baseline_size'];bz=r['candidate_size']
 ab=body('baseline',aa,az);bb=body('candidate',ba,bz)
 assert hashlib.sha256(ab).hexdigest()==r['baseline_sha256'] and hashlib.sha256(bb).hexdigest()==r['candidate_sha256']
 an,ash,af,am=normalize(ab,aa);bn,bsh,bf,bm=normalize(bb,ba)
 if not af or not bf:kind='incomplete_decode'
 elif ash!=bsh:kind='instruction_shape_differs'
 elif an==bn:kind='equal_after_masking_branch_and_rip_operands'
 else:kind='other_byte_differences'
 counts[kind]+=1;details.append(dict(name=r['name'],classification=kind,baseline_masked_bytes=am,candidate_masked_bytes=bm))
assert sum(counts.values())==len(rows)
out=dict(capstone_version=capstone.__version__,binary_sha256=summary['binary_sha256'],common_functions=len(rows),counts=dict(counts),semantic_equivalence_proved=False,causal_conclusion=False,performance_qualification=False)
(p/'summary.json').write_text(json.dumps(out,indent=2)+'\n')
(p/'classifications.json.gz').write_bytes(gzip.compress((json.dumps(details,separators=(',',':'))+'\n').encode(),mtime=0))
print(json.dumps(out))
