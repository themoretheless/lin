"""Native-only worker lifecycle/RSS measurement. Linux units; POSIX --preflight.

Do not name this script resource.py: os.wait4 imports stdlib resource while
constructing struct_rusage, so that name can re-execute a local script.
"""
from pathlib import Path
import argparse, hashlib, itertools, json, math, os, platform, statistics as st, subprocess, time, sys
import resource as rusage_module

assert hasattr(rusage_module, "getrusage") and hasattr(rusage_module, "struct_rusage")
if sys.argv[1:] == ["--preflight"]:
    child = subprocess.Popen([sys.executable, "-c", "pass"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    pid, status, usage = os.wait4(child.pid, 0)
    child.returncode = os.waitstatus_to_exitcode(status)
    assert pid == child.pid and child.returncode == 0
    assert isinstance(usage, rusage_module.struct_rusage) and usage.ru_maxrss > 0
    print("Native os.wait4 / stdlib resource preflight passed; no benchmark executed")
    raise SystemExit(0)

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--baseline-binary', type=Path, required=True)
parser.add_argument('--candidate-binary', type=Path, required=True)
args = parser.parse_args()
assert platform.system() == 'Linux', 'ru_maxrss below is defined in Linux KiB'
out = args.output.resolve()
out.mkdir(exist_ok=False)
bins = {'baseline': args.baseline_binary.resolve(), 'candidate': args.candidate_binary.resolve()}
hashes = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in bins.items()}
orders = list(itertools.permutations(['baseline', 'control', 'candidate']))
records = []
for round in range(1, 13):
    for n in [1000, 10000, 100000]:
        for role in orders[(round-1)%6]:
            binary_role = 'baseline' if role == 'control' else role
            binary = bins[binary_role]
            assert hashlib.sha256(binary.read_bytes()).hexdigest() == hashes[binary_role]
            stem = f'{round:02}-{n}-{role}'
            request = {'insert_rows': n, 'samples': 1, 'timestamp_ms': 123456}
            source = out/(stem+'.stdin')
            source.write_text(json.dumps(request)+'\n')
            output = out/(stem+'.json')
            with source.open() as stdin, output.open('w') as stdout, (out/(stem+'.stderr')).open('w') as stderr:
                start = time.perf_counter_ns()
                worker = subprocess.Popen([str(binary)], stdin=stdin, stdout=stdout, stderr=stderr)
                pid, status, usage = os.wait4(worker.pid, 0)
                elapsed = time.perf_counter_ns()-start
                worker.returncode = os.waitstatus_to_exitcode(status)
            assert pid == worker.pid and worker.returncode == 0
            response = json.loads(output.read_text())
            expected = [[f'd-{i}', f'bench://{i}', 'rag' if i%2 == 0 else 'sys',
                         f'doc {i} '+('wal note' if i%10 == 0 else 'plain'), 123456, f'body {i}'] for i in range(n)]
            assert response['result'] == expected
            assert len(response['samples_ns']) == 1 and math.isfinite(response['samples_ns'][0]) and response['samples_ns'][0] > 0
            assert elapsed > 0 and usage.ru_maxrss > 0
            values = response.pop('result')
            response['result_sha256'] = hashlib.sha256(json.dumps(values, separators=(',', ':')).encode()).hexdigest()
            response['validated_result_rows'] = n
            output.write_text(json.dumps(response)+'\n')
            records.append({'round': round, 'rows': n, 'role': role, 'file': output.name,
                            'exit': worker.returncode, 'binary_sha256': hashes[binary_role],
                            'process_wall_ns': elapsed, 'peak_rss_kib': usage.ru_maxrss,
                            'process_user_cpu_s': usage.ru_utime, 'process_system_cpu_s': usage.ru_stime})
            (out/'records.json').write_text(json.dumps(records, indent=2)+'\n')
            print(stem, 'cold lifecycle and RSS verified', flush=True)
assert len(records) == 108
summary = []
for n in [1000, 10000, 100000]:
    for metric, floor in [('process_wall_ns', 2), ('peak_rss_kib', 5)]:
        values = {(r['round'], r['role']): r[metric] for r in records if r['rows'] == n}
        for series, start in [(1, 1), (2, 7)]:
            gains = [100*(1-values[i, 'candidate']/values[i, 'baseline']) for i in range(start, start+6)]
            aa = [abs(100*(1-values[i, 'control']/values[i, 'baseline'])) for i in range(start, start+6)]
            gain, noise = st.median(gains), st.median(aa)
            summary.append(dict(rows=n, metric=metric, series=series, gains_percent=gains,
                                aa_absolute_percent=aa, median_gain_percent=gain, median_aa_percent=noise,
                                tolerance_percent=max(floor, noise), passes=gain >= -max(floor, noise)))
(out/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
(out/'decision.json').write_text(json.dumps({'complete': True, 'processes': 108, 'native_operation_samples': 108,
    'process_wall_samples': 108, 'peak_rss_observations': 108,
    'resource_gate_passes': all(r['passes'] for r in summary), 'full_objective_achieved': False}, indent=2)+'\n')
print('Failed resource windows:', json.dumps([r for r in summary if not r['passes']]), flush=True)
