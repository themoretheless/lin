"""Audit the frozen aggregate-source seed/query lifecycle resource gate; never executes binaries or timings."""
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
slug = 'benches/results/2026-11-03-orders-lifecycle-linux/'
spec = json.loads(blob(head, slug+'input.json'))
for name, digest in spec['files_sha256'].items():
    assert sha(blob(head, name)) == digest
assert spec['candidate_runtime_reference'] == 'b9f10bc776b05eb5235237e9a900c2ef6aaed1fd'
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
    assert (p/f'clean-{role}.log').is_file()
    build_log = re.sub(r'\x1b\[[0-9;]*m', '', (p/f'build-{role}.log').read_text())
    assert re.search(r'Compiling lin v[0-9]', build_log) and re.search(r'Compiling lin-peer-worker v[0-9]', build_log)
assert [name for name in spec['runtime_files'] if meta['builds']['candidate']['source_sha256'][name]
        != meta['builds']['baseline']['source_sha256'][name]] == ['src/exec.rs', 'src/store.rs']
log = args.workspace_log.read_text()
assert not re.search(r'test result: FAILED|could not compile|error\[E', log)
totals = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
assert totals and all(int(x[1]) == 0 for x in totals)
test_counts = [sum(int(row[i]) for row in totals) for i in range(3)]
assert test_counts[0] >= 414
assert spec['baseline_reference'] == 'a28aa758b774e216f14a6348fb1cf9dca2eb42bc'
records = load(p/'measurement/records.json')
assert len(records) == 108
coords=set()
for rec in records:
    r,n,role=(rec[k] for k in ['round','rows','role'])
    assert (r,n,role) not in coords and 1<=r<=12 and n in [1000,10000,100000] and role in ['baseline','control','candidate']
    coords.add((r,n,role));stem=f'{r:02}-{n}-{role}'
    assert rec['file']==stem+'.json'
    binrole='baseline' if role=='control' else role
    assert rec['binary_sha256']==meta['builds'][binrole]['binary_sha256']
    stdin=p/'measurement'/(stem+'.stdin')
    queries=[json.loads(line) for line in stdin.read_text().splitlines()]
    assert queries == [{'rows':n},{'source':'orders | sum total','fields':['total'],'samples':1,'iterations':1},{'source':'orders | {id,user_id,total} | take all','fields':['id','user_id','total'],'samples':1,'iterations':1}]
    data=[json.loads(line) for line in (p/'measurement'/rec['file']).read_text().splitlines()]
    assert len(data)==3 and data[0]['ready'] is True
    assert data[1]['result']==[[sum(i%200 for i in range(n))]]
    users=max(1,n//10)
    assert data[2]['result']==[[f'o-{i}',f'u-{i%users}',i%200] for i in range(n)]
    for response in data[1:]:
        assert len(response['samples_ns'])==1 and all(type(x) in [float,int] and math.isfinite(x) and x>0 for x in response['samples_ns'])
    assert not (p/'measurement'/(stem+'.stderr')).read_text()
    for key in ['process_wall_ns','peak_rss_kib']:
        assert type(rec[key]) in [float,int] and math.isfinite(rec[key]) and rec[key]>0
windows=[]
for n in [1000,10000,100000]:
    for metric,floor in [('process_wall_ns',2),('peak_rss_kib',5)]:
        v={(r['round'],r['role']):r[metric] for r in records if r['rows']==n}
        for series,start in [(1,1),(2,7)]:
            gains=[100*(1-v[r,'candidate']/v[r,'baseline']) for r in range(start,start+6)]
            aa=[abs(100*(1-v[r,'control']/v[r,'baseline'])) for r in range(start,start+6)]
            gain,noise=st.median(gains),st.median(aa)
            windows.append(dict(rows=n,metric=metric,series=series,gains_percent=gains,aa_absolute_percent=aa,median_gain_percent=gain,median_aa_percent=noise,tolerance_percent=max(floor,noise),passes=gain>=-max(floor,noise)))
assert windows == load(p/'measurement/summary.json')
decision=dict(complete=True,processes=108,lifecycle_resource_gate_passes=all(v['passes'] for v in windows),isolated_first_query_qualified=False,full_peer_qualified=False,full_objective_achieved=False)
assert decision == load(p/'measurement/decision.json')
args.output.write_text(json.dumps(dict(source_head=head,source_binaries_values_verified=True,workspace_counts=test_counts,processes=108,query_samples=216,decision=decision,windows=windows),indent=2)+'\n')
print('108 processes, 216 query samples and exact readback verified; failed windows:',sum(not v['passes'] for v in windows))
