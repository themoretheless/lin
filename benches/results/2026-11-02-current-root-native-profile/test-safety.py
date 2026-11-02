from pathlib import Path
import ast,json,os,subprocess,sys,tempfile
from safety_scan import scan_directory
p=Path(__file__).resolve().parent;m=json.loads((p/'input.json').read_text());env=m['profiled_environment'];assert set(env)=={'HOME','PATH','LANG','LC_ALL'}
# Synthetic values are constructed at runtime; no live credential is read.
synthetic=''.join(('gh','s_'))+('a'*36)
os.environ['GH_TOKEN']=synthetic
result=subprocess.check_output([sys.executable,'-c','import os,json;print(json.dumps(sorted(os.environ)))'],env=env,text=True)
child_keys=set(json.loads(result));automatic={'__CF_USER_TEXT_ENCODING'}if sys.platform=='darwin'else set()
assert set(env)<=child_keys and child_keys-set(env)<=automatic and 'GH_TOKEN'not in child_keys
with tempfile.TemporaryDirectory(prefix='lin-profile-safety-fixture-')as td:
 d=Path(td);(d/'safe.bin').write_bytes(b'ordinary fake benchmark rows and addresses\x00');assert all(v==0 for counts in scan_directory(d).values()for v in counts.values())
 assignment='_'.join(('GH','TOKEN'))+'='+synthetic
 jwt='.'.join((''.join(('ey','J'))+('a'*24),'b'*24,'c'*24))
 (d/'unsafe.bin').write_bytes((assignment+'\x00'+jwt).encode());counts=scan_directory(d)['unsafe.bin'];assert all(v>0 for v in counts.values())
source=ast.parse((p/'profile.py').read_text());checked=0
for node in ast.walk(source):
 if not isinstance(node,ast.Call)or not isinstance(node.func,ast.Attribute)or not isinstance(node.func.value,ast.Name)or node.func.value.id!='subprocess'or node.func.attr!='run':continue
 first=node.args[0]
 if isinstance(first,ast.List)and isinstance(first.elts[0],ast.Constant)and first.elts[0].value=='gh':continue
 supplied=next((x.value for x in node.keywords if x.arg=='env'),None);assert isinstance(supplied,ast.Name)and supplied.id=='profile_env';checked+=1
assert checked==2
workflow=(p.parents[2]/'.github/workflows/current-root-native-profile.yml').read_text();assert "if: ${{ success() && steps.profile.outcome == 'success' }}"in workflow and 'current-root-native-profile-private-failure'in workflow
print('Safety PASS: child environment excludes controller credential, all three synthetic secret classes rejected, public upload requires success, failed evidence encrypted')
