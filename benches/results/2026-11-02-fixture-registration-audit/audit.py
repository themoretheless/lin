import ast,collections,hashlib,json,pathlib,struct,subprocess,sys
import capstone
from capstone.x86 import X86_OP_MEM,X86_REG_RIP,X86_OP_IMM
p=pathlib.Path(__file__).resolve().parent
prior=p.parent/'2026-11-02-duckdb-address-operand-audit'
source=(prior/'audit.py').read_text();tree=ast.parse(source)
node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='normalize')
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64);md.detail=True
exec(compile(ast.Module(body=[node],type_ignores=[]),'verified-normalizer','exec'))
expected=json.loads((prior/'summary.json').read_text())['binary_sha256']
root=pathlib.Path(sys.argv[1]);binaries={};segments={};symbols={}
for role,digest in expected.items():
 f=root/f'lin-corrected-full-engine-{role}';b=f.read_bytes();assert hashlib.sha256(b).hexdigest()==digest
 off=struct.unpack_from('<Q',b,32)[0];size,num=struct.unpack_from('<HH',b,54);loads=[]
 for i in range(num):
  kind,flags,offset,addr,_,filesz,_,_=struct.unpack_from('<IIQQQQQQ',b,off+i*size)
  if kind==1 and flags&1:loads.append((addr,addr+filesz,offset))
 binaries[role]=b;segments[role]=loads
 txt=subprocess.check_output(['objdump','--syms','--demangle',str(f)],text=True)
 d=collections.defaultdict(list)
 for line in txt.splitlines():
  parts=line.split(None,5)
  if len(parts)==6 and parts[2]=='F' and ('airbug_bench::convenience::Fixture$LT$T$GT$::new::' in parts[5] or parts[5].startswith('.hidden compare::main::h')):
   d[parts[5]].append((int(parts[0],16),int(parts[4],16)))
 assert all(len(v)==1 for v in d.values())
 symbols[role]={k:v[0] for k,v in d.items()}
def body(role,addr,size):
 for low,high,off in segments[role]:
  if low<=addr and addr+size<=high:return binaries[role][off+addr-low:off+addr-low+size]
 raise AssertionError((role,addr,size))
a,b=symbols['baseline'],symbols['candidate'];assert set(a)==set(b)
records=[]
for name in sorted(a):
 per={};norm={}
 for role in ['baseline','candidate']:
  addr,size=symbols[role][name];code=body(role,addr,size);n,shape,full,masked=normalize(code,addr)
  assert full
  constants=[]
  for ins in md.disasm(code,addr):
   if ins.group(capstone.CS_GRP_CALL) or ins.group(capstone.CS_GRP_JUMP):continue
   if any(op.type==X86_OP_IMM for op in ins.operands):constants.append(dict(offset=ins.address-addr,mnemonic=ins.mnemonic,operands=ins.op_str))
  norm[role]=(n,shape)
  per[role]=dict(address=addr,size=size,raw_sha256=hashlib.sha256(code).hexdigest(),normalized_sha256=hashlib.sha256(n).hexdigest(),nonbranch_immediates=constants)
 records.append(dict(name=name,normalized_equal=norm['baseline']==norm['candidate'],**per))
result=dict(binary_sha256=expected,normalizer_source_sha256=hashlib.sha256(source.encode()).hexdigest(),functions=len(records),normalized_equal=sum(r['normalized_equal'] for r in records),records=records,causal_conclusion=False,performance_qualification=False,scope='14 out-of-line Fixture::new bodies plus main; indirect callees, runtime allocator state and dynamic initialization are not proven')
(p/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='records'}))
