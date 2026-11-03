from pathlib import Path
import ast,importlib.util,json,hashlib
p=Path(__file__).resolve().parent;spec=importlib.util.spec_from_file_location('host_guard',p/'check-host.py');guard=importlib.util.module_from_spec(spec);spec.loader.exec_module(guard)
for release,bad in [('6.8.0-1070-azure',False),('6.19.0',True),('7.0.0-1017-azure',True),('7.0.13',True),('7.0.14',False)]:assert guard.kernel_affected(release)==bad
for release in ['unknown','7.1.0-rc1']:
 try:guard.kernel_affected(release)
 except ValueError:pass
 else:raise AssertionError('unrecognized or prerelease kernel accepted')
for f in p.rglob('*.py'):ast.parse(f.read_text())
for mode in ['read','native']:
 s=(p/mode/'measure.py').read_text();assert '--samples\',\'12' in s and '--repeats\',\'3' in s and '--require-wins' in s
 assert '/tmp/lin-current-candidate-public-peers-20261103/'+mode in s and 'sentinels_unchanged' in s
s=(p/'build.py').read_text();assert "'--locked','--release','-p','lin-peer-worker'" in s and '--features' not in s
s=(p/'run.py').read_text();assert "str(assets/mode/'measure.py')"in s and "metadata['created_container_ids']"in s and "assert not errors"in s
print('Synthetic/static protocol checks passed: guarded kernel, fixed cases, samples, executable and asset/output separation')
