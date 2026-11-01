from pathlib import Path
import hashlib
import json
import math
import runpy
import shutil
import subprocess
import sys
import tempfile
sys.path.insert(0, str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule

src = Path(__file__).resolve().parent
full_src = src.parent/'2026-11-01-corrected-cow-full-engine'
verify = runpy.run_path(str(src/'verify.py'))['verify']
roles = ['baseline_a', 'candidate_a', 'baseline_b', 'candidate_b', 'baseline_c', 'candidate_c']
with tempfile.TemporaryDirectory(prefix='lin-crossed-control-synthetic-') as td:
    p = Path(td)/src.name
    p.mkdir()
    full = p.parent/full_src.name
    full.mkdir()
    for name in ['source-inputs.json', 'compare.rs', 'cases.json']:
        shutil.copy2(full_src/name, full/name)
    for name in ['cases.json', 'frozen.json']:
        shutil.copy2(src/name, p/name)
    write = lambda name, value: (p/name).write_text(json.dumps(value, indent=2)+'\n')
    source = json.loads((full/'source-inputs.json').read_text())
    digest = lambda f: hashlib.sha256(f.read_bytes()).hexdigest()
    binaries = {}
    for role in ['baseline', 'candidate']:
        f = p/(role+'-synthetic-binary'); f.write_bytes(role.encode())
        binaries[role] = dict(path=str(f), sha256=digest(f), cargo_target_dir='target/corrected-full-engine-'+role, source_sha256=source[role]['source_sha256'])
    write('provenance.json', dict(synthetic_fixture=True, instrumentation=False, source_restored=True,
          source_sha256=source['candidate']['source_sha256'], original_compare_sha256=digest(Path('benches/compare.rs')),
          compiled_compare_sha256=digest(full/'compare.rs'), binaries=binaries))
    write('environment.json', dict(platform='Linux-synthetic-fixture', runtime_unchanged_all_roles=True))
    smokes = []
    for role in binaries:
        for case in json.loads((full/'cases.json').read_text()):
            dest = full/'local-smoke'/role/case.replace('/', '_'); dest.mkdir(parents=True)
            (dest/'run.json').write_text(json.dumps(dict(status='complete', observations=[dict(case=case, operations=1)])))
            smokes.append(dict(role=role, case=case, exit_code=0))
    (full/'local-smoke-status.json').write_text(json.dumps(dict(runs=smokes)))
    root = p/'measurement'; root.mkdir()
    records = []
    cases = json.loads((p/'cases.json').read_text())
    for item in schedule(cases, roles):
        rnd, case, role = item['round'], item['workload'], item['role']
        source_role = role.split('_')[0]; binary = binaries[source_role]
        name = f'{rnd:02}-{role}-'+case.replace('/', '_'); dest = root/name; dest.mkdir()
        value = 100 if source_role == 'baseline' else 120
        observation = dict(case=case, metric='wall', operations=1, value=value, availability=dict(state='available'))
        d = dict(schema=1, status='complete', id=name, cases=[dict(id=case)], observations=[observation]*8,
                 environment=dict(arch='x86_64', os='linux'), provenance=dict(samples='8', warmup_ns='0'))
        (dest/'run.json').write_text(json.dumps(d))
        resource = root/(name+'-resource.txt'); resource.write_text('Maximum resident set size (kbytes): 123\n')
        command = ['/usr/bin/time', '-v', '-o', str(resource), '--', binary['path'], '--filter', case,
                   '--samples', '8', '--max-iterations', '1', '--warmup-ms', '0', '--sample-ms', '1', '--output', str(dest)]
        records.append(dict(round=rnd, case=case, role=role, source_role=source_role, binary_sha256=binary['sha256'], command=command, output=name, exit_code=0))
    def analyze(ok=True):
        r = subprocess.run(['python3', str(src/'analyze.py'), str(p)], capture_output=True, text=True)
        assert (r.returncode == 0) == ok, (r.stdout, r.stderr)
    def audit(ok=True):
        try:
            result = verify(p, allow_synthetic=True)
        except (AssertionError, KeyError, ValueError):
            assert not ok
        else:
            assert ok and result['synthetic_fixture'] and not result['performance_qualification']
    write('measurement/process-status.json', records)
    analyze(); audit()
    summary = json.loads((p/'summary.json').read_text())
    assert all(math.isclose(r['cross_build_gain_percent'], -20) and r['positive_rounds'] == 0 for v in summary.values() for r in v.values())
    assert all(n['gain_percent'] == 0 for v in summary.values() for r in v.values() for n in r['nulls'].values())
    # Production entry point must refuse a synthetic provenance label.
    try: verify(p)
    except AssertionError: pass
    else: raise AssertionError('Synthetic fixture accepted as actual evidence')
    original = (p/'measurement/process-status.json').read_text()
    for mutation in ['missing', 'order', 'command', 'hash', 'source_role', 'old_schedule']:
        bad = json.loads(original)
        if mutation == 'missing': bad.pop()
        elif mutation == 'order': bad[0], bad[1] = bad[1], bad[0]
        elif mutation == 'command': bad[0]['command'][bad[0]['command'].index('--warmup-ms')+1] = '1'
        elif mutation == 'hash': bad[0]['binary_sha256'] = '0'*64
        elif mutation == 'source_role': bad[0]['source_role'] = 'candidate' if bad[0]['source_role'] == 'baseline' else 'baseline'
        else:
            by_key = {(r['round'], r['case'], r['role']): r for r in bad}; bad = []
            ids = [c['id'] for c in cases]
            for rnd in range(1, 13):
                for i, case in enumerate(ids[rnd % 3:]+ids[:rnd % 3]):
                    shift = (rnd+i) % 6
                    bad.extend(by_key[rnd, case, role] for role in roles[shift:]+roles[:shift])
        write('measurement/process-status.json', bad); audit(False)
        if mutation in ['missing', 'order', 'old_schedule']: analyze(False)
    (p/'measurement/process-status.json').write_text(original)
    first = root/records[0]['output']/'run.json'; original_run = first.read_text()
    for bad_value in [float('nan'), 0, -1]:
        d = json.loads(original_run); d['observations'][0]['value'] = bad_value; first.write_text(json.dumps(d)); analyze(False); audit(False)
    d = json.loads(original_run); d['observations'].pop(); first.write_text(json.dumps(d)); analyze(False); audit(False)
    first.write_text(original_run)
    original_summary = (p/'summary.json').read_text()
    bad = json.loads(original_summary); bad[cases[0]['id']]['series1']['pairs'][0]['cross_build_gain_percent'] = 99
    write('summary.json', bad); audit(False); (p/'summary.json').write_text(original_summary)
    # An actual null-label difference must be reported, not discarded.
    for r in records:
        if r['role'] == 'candidate_b':
            f = root/r['output']/'run.json'; d = json.loads(f.read_text())
            for o in d['observations']: o['value'] = 132
            f.write_text(json.dumps(d))
    analyze(); audit()
    result = json.loads((p/'summary.json').read_text())
    assert all(math.isclose(r['nulls']['candidate_ab']['gain_percent'], -10) for v in result.values() for r in v.values())
print('PASS synthetic crossed audit: complete losing result and null drift retained; old order, missing data, commands, identity, NaN and summary corruptions rejected; no actual timing qualification')
