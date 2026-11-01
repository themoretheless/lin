from pathlib import Path
import hashlib
import json
import math
import re
import sys
sys.path.insert(0, str(Path.cwd()/'scripts'))
from benchmark_role_balance import orders_from_records, role_order

def verify(p, allow_synthetic=False):
    read = lambda name: json.loads((p/name).read_text())
    digest = lambda f: hashlib.sha256(Path(f).read_bytes()).hexdigest()
    middle = lambda xs: (sorted(xs)[(len(xs)-1)//2]+sorted(xs)[len(xs)//2])/2
    meta = read('provenance.json')
    assert not meta.get('synthetic_fixture') or allow_synthetic
    frozen = read('frozen.json')
    assert all(digest(name) == h for name, h in frozen.items())
    full = p.parent/'2026-11-01-corrected-cow-full-engine'
    source = json.loads((full/'source-inputs.json').read_text())
    assert meta['source_restored'] and not meta['instrumentation']
    assert meta['source_sha256'] == source['candidate']['source_sha256']
    assert all(digest(name) == h for name, h in meta['source_sha256'].items())
    assert digest('benches/compare.rs') == meta['original_compare_sha256']
    assert digest(full/'compare.rs') == meta['compiled_compare_sha256']
    for role in ['baseline', 'candidate']:
        b = meta['binaries'][role]
        assert b['source_sha256'] == source[role]['source_sha256']
        assert b['cargo_target_dir'] == 'target/corrected-full-engine-'+role
        assert digest(b['path']) == b['sha256']
    env = read('environment.json')
    assert env['runtime_unchanged_all_roles'] and 'Linux' in env['platform']
    smokes = json.loads((full/'local-smoke-status.json').read_text())
    all_cases = json.loads((full/'cases.json').read_text())
    assert len(smokes['runs']) == 134 and {(r['role'], r['case']) for r in smokes['runs']} == {(role, case) for role in ['baseline', 'candidate'] for case in all_cases}
    for r in smokes['runs']:
        d = json.loads((full/'local-smoke'/r['role']/r['case'].replace('/', '_')/'run.json').read_text())
        assert r['exit_code'] == 0 and d['status'] == 'complete' and len(d['observations']) == 1
        assert d['observations'][0]['case'] == r['case'] and d['observations'][0]['operations'] == 1
    cases = [c['id'] for c in read('cases.json')]
    assert cases == ['compare/agg_count_by/duckdb', 'compare/durable_delete_1row_10k/sqlite_delete', 'compare/durable_commit_1row_1k/lin']
    roles = ['baseline_a', 'candidate_a', 'baseline_b', 'candidate_b', 'baseline_c', 'candidate_c']
    records = read('measurement/process-status.json')
    expected = []
    for rnd in range(1, 13):
        for case in cases[rnd % 3:]+cases[:rnd % 3]:
            expected.extend((rnd, case, role) for role in role_order(rnd, cases.index(case), roles))
    assert len(records) == 216 and [(r['round'], r['case'], r['role']) for r in records] == expected
    orders_from_records(records, cases, roles, 'case')
    med = {}
    ids = set()
    for r in records:
        source_role = r['role'].split('_')[0]
        assert r['source_role'] == source_role and r['exit_code'] == 0
        b = meta['binaries'][source_role]
        assert r['binary_sha256'] == b['sha256']
        name = f"{r['round']:02}-{r['role']}-"+r['case'].replace('/', '_')
        assert r['output'] == name
        command = r['command']
        assert command[:3] == ['/usr/bin/time', '-v', '-o']
        resource = Path(command[3]); dest = Path(command[-1])
        assert dest == p/'measurement'/name
        assert resource == dest.parent/(name+'-resource.txt') and dest.name == name
        assert command[4:] == ['--', b['path'], '--filter', r['case'], '--samples', '8', '--max-iterations', '1', '--warmup-ms', '0', '--sample-ms', '1', '--output', str(dest)]
        assert re.search(r'Maximum resident set size \(kbytes\):\s*[1-9][0-9]*', (p/'measurement'/(name+'-resource.txt')).read_text())
        d = read('measurement/'+name+'/run.json')
        assert d['schema'] == 1 and d['status'] == 'complete' and d['id'] not in ids
        ids.add(d['id'])
        assert d['environment'] == dict(arch='x86_64', os='linux')
        assert d['provenance']['samples'] == '8' and d['provenance']['warmup_ns'] == '0'
        assert len(d['cases']) == 1 and d['cases'][0]['id'] == r['case'] and len(d['observations']) == 8
        values = []
        for o in d['observations']:
            v = float(o['value'])
            assert math.isfinite(v) and v > 0 and o['case'] == r['case']
            assert o['metric'] == 'wall' and o['operations'] == 1 and o['availability']['state'] == 'available'
            values.append(v)
        med[r['round'], r['case'], r['role']] = middle(values)
    summary = read('summary.json')
    assert set(summary) == set(cases)
    for case in cases:
        assert set(summary[case]) == {'series1', 'series2'}
        for series, start in [('series1', 1), ('series2', 7)]:
            result = summary[case][series]; pairs = result['pairs']
            assert len(pairs) == 6
            for rnd, pair in zip(range(start, start+6), pairs):
                ns = {role: med[rnd, case, role] for role in roles}
                a = middle([ns['baseline_'+suffix] for suffix in 'abc'])
                b = middle([ns['candidate_'+suffix] for suffix in 'abc'])
                nulls = {source+'_'+left+right: 100*(1-ns[source+'_'+right]/ns[source+'_'+left]) for source in ['baseline', 'candidate'] for left, right in [('a', 'b'), ('a', 'c'), ('b', 'c')]}
                assert pair == dict(round=rnd, role_ns=ns, baseline_group_ns=a, candidate_group_ns=b, cross_build_gain_percent=100*(1-b/a), null_gains_percent=nulls)
            assert result['cross_build_gain_percent'] == middle([r['cross_build_gain_percent'] for r in pairs])
            assert result['positive_rounds'] == sum(r['cross_build_gain_percent'] > 0 for r in pairs)
            expected_nulls = {key: dict(gain_percent=middle([r['null_gains_percent'][key] for r in pairs]), positive_rounds=sum(r['null_gains_percent'][key] > 0 for r in pairs)) for key in pairs[0]['null_gains_percent']}
            assert result['nulls'] == expected_nulls
    status = read('diagnostic-status.json')
    assert status == dict(complete=True, processes=216, observations=1728, runtime_adopted=False, performance_qualification=False, full_objective_achieved=False)
    return dict(complete=True, processes=216, observations=1728, raw_commands_verified=True,
                identical_binary_nulls_verified=True, source_hashes_verified=True, balanced_order_verified=True,
                synthetic_fixture=bool(meta.get('synthetic_fixture')), performance_qualification=False, full_objective_achieved=False)

if __name__ == '__main__':
    p = Path(__file__).resolve().parent
    result = verify(p)
    (p/'independent-audit.json').write_text(json.dumps(result, indent=2)+'\n')
    print('PASS crossed source, command, binary identity, order and numerical audit')
