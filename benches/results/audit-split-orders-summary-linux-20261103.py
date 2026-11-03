"""Audit the frozen isolated orders mutation and first-summary gate; never executes binaries or timings."""
from pathlib import Path
import argparse, hashlib, json, math, re, statistics as st, subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--raw', type=Path, required=True)
parser.add_argument('--workspace-log', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--expected-head', required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
p = args.raw
sha = lambda data: hashlib.sha256(data).hexdigest()
load = lambda path: json.loads(path.read_text())
def blob(ref, name):
    return subprocess.check_output(['git', 'show', ref+':'+name], cwd=root)
meta = load(p/'build-provenance.json')
assert meta['measurement_exit'] == 0 and meta['source_unchanged']
assert meta['status'] == 'finished'
head = meta['head']
assert re.fullmatch(r'[0-9a-f]{40}', head) and head == args.expected_head
slug = 'benches/results/2026-11-03-split-orders-summary-linux/'
spec = json.loads(blob(head, slug+'input.json'))
for name, digest in spec['files_sha256'].items():
    assert sha(blob(head, name)) == digest
assert spec['candidate_runtime_reference'] == 'ec5dfd34ef21309c2a31d8d41fe684c67a6b4d86'
for name in spec['runtime_files']:
    assert blob(head, name) == blob(spec['candidate_runtime_reference'], name)
assert 'scripts/bench-native-lifecycle.py' in spec['files_sha256']
assert sorted(spec['runtime_files']) == sorted(spec['rust_source_files']+[
    'Cargo.toml', 'Cargo.lock', 'lin-derive/Cargo.toml', 'tools/peer-worker/Cargo.toml'])
assert sorted(meta['builds']) == ['baseline', 'candidate']
for role, build in meta['builds'].items():
    ref = head if role == 'candidate' else spec['baseline_reference']
    assert build['reference'] == ref
    assert set(build['source_sha256']) == set(spec['runtime_files'])
    for name, digest in build['source_sha256'].items():
        assert sha(blob(ref, name)) == digest
    binary = (p/f'{role}-worker').read_bytes()
    assert binary[:6] == b'\x7fELF\x02\x01' and int.from_bytes(binary[18:20], 'little') == 62
    assert sha(binary) == build['binary_sha256']
    assert build['harness_sha256'] == spec['files_sha256']['examples/orders_probe.rs']
    assert (p/f'clean-{role}.log').is_file()
    build_log = re.sub(r'\x1b\[[0-9;]*m', '', (p/f'build-{role}.log').read_text())
    assert re.search(r'Compiling lin v[0-9]', build_log) and re.search(r'--crate-name orders_probe ', build_log)
assert [name for name in spec['runtime_files'] if meta['builds']['candidate']['source_sha256'][name]
        != meta['builds']['baseline']['source_sha256'][name]] == ['src/exec.rs', 'src/store.rs']
log = args.workspace_log.read_text()
assert 'sum_and_numeric_safety_caches_are_independently_lazy ... ok' in log
assert not re.search(r'test result: FAILED|could not compile|error\[E', log)
totals = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
assert totals and all(int(x[1]) == 0 for x in totals)
test_counts = [sum(int(row[i]) for row in totals) for i in range(3)]
assert test_counts[0] >= 415
assert spec['baseline_reference'] == 'a28aa758b774e216f14a6348fb1cf9dca2eb42bc'
PHASES=['insert','first_sum','warm_sum','update','after_update_sum','delete','after_delete_sum']
records=load(p/'measurement/records.json');assert len(records)==108
coords=set()
for rec in records:
    r,n,role=(rec[k] for k in ['round','rows','role'])
    assert (r,n,role) not in coords and 1<=r<=12 and n in [1000,10000,100000] and role in ['baseline','control','candidate']
    coords.add((r,n,role));stem=f'{r:02}-{n}-{role}'
    assert rec['file']==stem+'.json'
    binrole='baseline' if role=='control' else role
    assert rec['binary_sha256']==meta['builds'][binrole]['binary_sha256']
    data=load(p/'measurement'/rec['file'])
    assert data['rows']==n and data['statement_rows_limit']==500 and len(data['samples'])==1
    sample=data['samples'][0]
    assert set(sample['phase_ns'])==set(PHASES) and rec['phase_ns']==sample['phase_ns']
    assert all(type(v) in [int,float] and math.isfinite(v) and v>0 for v in sample['phase_ns'].values())
    total=sum(i%200 for i in range(n))
    assert sample['sums']==[total,total+201,total+201-(n-1)%200]
    states=[]
    for updated,deleted in [(False,False),(True,False),(True,True)]:
        states.append([[f'o-{i}','u',201 if updated and i==0 else i%200] for i in range(n-int(deleted))])
    assert sample['states']==states
    assert not (p/'measurement'/(stem+'.stderr')).read_text()
    for key in ['process_wall_ns','peak_rss_kib']:
        assert type(rec[key]) in [int,float] and math.isfinite(rec[key]) and rec[key]>0
windows=[]
for n in [1000,10000,100000]:
    for metric in PHASES+['process_wall_ns','peak_rss_kib']:
        values={(rec['round'],rec['role']):(rec['phase_ns'][metric] if metric in PHASES else rec[metric]) for rec in records if rec['rows']==n}
        for series,start in [(1,1),(2,7)]:
            gains=[100*(1-values[r,'candidate']/values[r,'baseline']) for r in range(start,start+6)]
            aa=[abs(100*(1-values[r,'control']/values[r,'baseline'])) for r in range(start,start+6)]
            gain,noise=st.median(gains),st.median(aa);positive=sum(v>0 for v in gains);target=metric=='warm_sum';tolerance=max(5 if metric=='peak_rss_kib' else 2,noise)
            passed=(positive==6 and gain>noise) if target else gain>=-tolerance
            windows.append(dict(rows=n,metric=metric,series=series,target=target,gains_percent=gains,aa_absolute_percent=aa,median_gain_percent=gain,median_aa_percent=noise,positive_pairs=positive,tolerance_percent=tolerance,passes=passed))
assert windows==load(p/'measurement/summary.json')
decision=dict(complete=True,processes=108,phase_samples=756,source_mutation_gate_passes=all(v['passes'] for v in windows),full_peer_qualified=False,full_objective_achieved=False)
assert decision==load(p/'measurement/decision.json')
args.output.write_text(json.dumps(dict(source_head=head,source_binaries_values_verified=True,workspace_counts=test_counts,decision=decision,windows=windows),indent=2)+'\n')
print('108 processes / 756 isolated phases / 324 complete states verified; failed windows:',sum(not w['passes'] for w in windows))
