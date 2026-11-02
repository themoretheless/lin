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
print(json.dumps(dict(kernel_boundaries=8,unknown_or_prerelease_rejected=2,docker_started=False,benchmark_timings=0)))
