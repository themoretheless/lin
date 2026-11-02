import ast,json,pathlib
import capstone
from capstone.x86 import X86_OP_MEM,X86_REG_RIP
p=pathlib.Path(__file__).resolve().parent
tree=ast.parse((p/'audit.py').read_text())
node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='normalize')
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_64);md.detail=True
exec(compile(ast.Module(body=[node],type_ignores=[]),'actual-audit-normalizer','exec'))
a=normalize(bytes.fromhex('e801000000'),0x1000);b=normalize(bytes.fromhex('e8ff000000'),0x2000)
assert a[:2]==b[:2] and a[2] and b[2]
a=normalize(bytes.fromhex('488b0501000000'),0x1000);b=normalize(bytes.fromhex('488b05ff000000'),0x2000)
assert a[:2]==b[:2] and a[2] and b[2]
a=normalize(bytes.fromhex('4883ec10'),0x1000);b=normalize(bytes.fromhex('4883ec20'),0x2000)
assert a[0]!=b[0] and a[1]==b[1] and a[2] and b[2], 'arithmetic immediates must remain visible'
a=normalize(bytes.fromhex('488b0501000000'),0x1000);b=normalize(bytes.fromhex('488d0501000000'),0x2000)
assert a[1]!=b[1], 'opcode change must remain visible'
assert not normalize(bytes.fromhex('0f'),0x1000)[2], 'truncated instruction must not pass'
a=normalize(bytes.fromhex('660f2e0501b26bff'),0x1000);b=normalize(bytes.fromhex('660f2e0521e86aff'),0x2000)
assert a[:2]==b[:2] and a[2] and b[2], 'SSE RIP disp32 must be fully covered'
result=dict(capstone_version=capstone.__version__,positive_address_operand_pairs=3,nonaddress_difference_controls=2,truncated_decode_rejected=True,passed=True)
(p/'normalizer-tests.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
