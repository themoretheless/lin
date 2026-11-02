from pathlib import Path
import hashlib
import json
import runpy
import shutil
import subprocess
import sys
import tempfile
sys.path.insert(0, str(Path.cwd()/'scripts'))
from benchmark_role_balance import schedule

src = Path(__file__).resolve().parent
verify = runpy.run_path(str(src/'verify.py'))['verify']
roles = ['baseline', 'control', 'candidate']
with tempfile.TemporaryDirectory(prefix='lin-fts-impact-synthetic-') as td:
    p = Path(td)
    for name in ['cases.json', 'input.json', 'compare.rs', 'frozen.json']:
        shutil.copy2(src/name, p/name)
    write = lambda name, value: (p/name).write_text(json.dumps(value, indent=2)+'\n')
    m = json.loads((p/'input.json').read_text()); cases = json.loads((p/'cases.json').read_text())
    binaries = {}
    for role in ['baseline', 'candidate']:
        binary = p/(role+'-synthetic-binary'); binary.write_bytes(role.encode())
        binaries[role] = dict(path=str(binary), sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), source_sha256=m[role]['source_sha256'], cargo_target_dir=str(p/'synthetic-target'))
    write('provenance.json', dict(source_restored=True, instrumentation=False, source_sha256=m['candidate']['source_sha256'],
        original_compare_sha256=hashlib.sha256(Path('benches/compare.rs').read_bytes()).hexdigest(),
        compiled_compare_sha256=hashlib.sha256((p/'compare.rs').read_bytes()).hexdigest(), binaries=binaries))
    write('environment.json', dict(synthetic_fixture=True, platform='Linux-synthetic', same_runtime_all_roles=True))
    smokes = []
    for role in ['baseline', 'candidate']:
        for case in cases:
            dest = p/'qualification-smoke'/(role+'-'+case.replace('/', '_')); dest.mkdir(parents=True)
            (dest/'run.json').write_text(json.dumps(dict(status='complete', cases=[dict(id=case)], observations=[dict(case=case)])))
            smokes.append(dict(role=role, case=case, exit=0))
    write('qualification-smoke-status.json', smokes)
    records = []
    for item in schedule([dict(id=c) for c in cases], roles):
        rnd, role, case = item['round'], item['role'], item['workload']
        binary = binaries['baseline' if role == 'control' else role]
        dest = p/f'{rnd:02}-{role}'/case.replace('/', '_'); dest.mkdir(parents=True)
        command = [binary['path'], '--filter', case, '--samples', '8', '--max-iterations', '1', '--warmup-ms', '0', '--sample-ms', '1', '--output', str(dest)]
        value = 102 if role == 'control' else 80 if role == 'candidate' and case in cases[:5] else 100
        o = dict(case=case, metric='wall', operations=1, availability=dict(state='available'), value=value)
        d = dict(schema=1, id=f'{rnd}-{role}-{case}', status='complete', cases=[dict(id=case)], observations=[o]*8, provenance=dict(samples='8', warmup_ns='0'))
        (dest/'run.json').write_text(json.dumps(d))
        records.append(dict(round=rnd, mode=role, case=case, exit_code=0, command=command, binary_sha256=binary['sha256']))
    write('process-status.json', records)
    def analyze(ok=True):
        result = subprocess.run(['python3', str(src/'analyze.py'), str(p)], capture_output=True, text=True)
        assert (result.returncode == 0) == ok, (result.stdout, result.stderr)
    def audit(ok=True):
        try: result = verify(p, allow_synthetic=True)
        except (AssertionError, KeyError, ValueError): assert not ok
        else: assert ok and result['synthetic_fixture'] and not result['full_objective_achieved']
    analyze(); audit()
    assert json.loads((p/'qualification.json').read_text())['component_gate_passes']
    try: verify(p)
    except AssertionError: pass
    else: raise AssertionError('Synthetic fixture accepted as actual evidence')
    original = (p/'process-status.json').read_text()
    for mutation in ['missing', 'order', 'old_order', 'command', 'hash']:
        bad = json.loads(original)
        if mutation == 'missing': bad.pop()
        elif mutation == 'order': bad[0], bad[1] = bad[1], bad[0]
        elif mutation == 'command': bad[0]['command'][bad[0]['command'].index('--samples')+1] = '7'
        elif mutation == 'hash': bad[0]['binary_sha256'] = '0'*64
        else:
            by_key = {(r['round'], r['mode'], r['case']): r for r in bad}; bad = []
            for rnd in range(1, 13):
                for i, c in enumerate(cases[rnd % 20:]+cases[:rnd % 20]):
                    shift = (rnd+i) % 3
                    bad.extend(by_key[rnd, role, c] for role in roles[shift:]+roles[:shift])
        write('process-status.json', bad); audit(False)
        if mutation in ['missing', 'order', 'old_order']: analyze(False)
    (p/'process-status.json').write_text(original)
    summary_text = (p/'summary.json').read_text()
    bad = json.loads(summary_text); bad[cases[0]]['series1']['gain_percent'] = 999
    write('summary.json', bad); audit(False); (p/'summary.json').write_text(summary_text)
    f = p/'01-candidate'/cases[0].replace('/', '_')/'run.json'; original_run = f.read_text()
    d = json.loads(original_run); d['observations'][0]['value'] = float('nan'); f.write_text(json.dumps(d)); analyze(False); audit(False)
    d = json.loads(original_run); d['observations'].pop(); f.write_text(json.dumps(d)); analyze(False); audit(False)
    d = json.loads(original_run)
    for o in d['observations']: o['value'] = 120
    f.write_text(json.dumps(d)); analyze(); audit()
    assert not json.loads((p/'qualification.json').read_text())['component_gate_passes'], 'Losing target pair must fail'
    f.write_text(original_run)
    for rnd in range(1, 7):
        f = p/f'{rnd:02}-candidate'/cases[5].replace('/', '_')/'run.json'; d = json.loads(f.read_text())
        for o in d['observations']: o['value'] = 130
        f.write_text(json.dumps(d))
    analyze(); audit()
    assert not json.loads((p/'qualification.json').read_text())['component_gate_passes'], 'Regressing control must fail'
print('PASS synthetic 720-process protocol; old order and corruptions rejected, losing target/control retained; no actual speed qualification')
