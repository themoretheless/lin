import importlib.util,json
from pathlib import Path
p=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('mongo_readiness_probe',p/'probe.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
for release,expected in [('6.8.0-1070-azure',False),('6.18.99-generic',False),('6.19.0-generic',True),('6.19.99-generic',True),('7.0.0-1017-azure',True),('7.0.13-generic',True),('7.0.14-generic',False),('7.1.0-generic',False)]:
    assert module.kernel_affected(release)==expected,release
for release in ['unknown','7.0.14-rc1']:
    try:module.kernel_affected(release)
    except ValueError:pass
    else:raise AssertionError(release)
for token in ['0'*32,'a'*32,'0123456789abcdef'*2]:
    owner='lin-mongo-ubuntu22-'+token
    assert len(owner.replace('-','_'))+len('lin_readiness_')==65
    assert module.database_name(owner)=='lin_ready_'+token and len(module.database_name(owner))==42
for owner in ['bad-owner','lin-mongo-ubuntu22-'+'f'*33]:
    try:module.database_name(owner)
    except AssertionError:pass
    else:raise AssertionError(owner)
print(json.dumps(dict(kernel_boundaries=8,unknown_or_prerelease_rejected=2,database_name_fixtures=3,invalid_owner_rejected=2,docker_started=False,benchmark_timings=0)))
