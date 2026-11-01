"""Audit downloaded Linux correctness artifacts before allowing matrix builds."""
from pathlib import Path
import hashlib, json, re, subprocess

p = Path(__file__).resolve().parent
frozen = json.loads((p / 'source-inputs.json').read_text())
sha = lambda data: hashlib.sha256(data).hexdigest()
def blob(ref, name):
    return subprocess.check_output(['git', 'show', ref + ':' + name])

results = {}
for role, count, mutations, result_name in [
    ('baseline', 441, 6, '2026-11-01-btreemap-correctness'),
    ('candidate', 458, 5, '2026-11-01-corrected-update-patch'),
]:
    root = p / 'correctness-artifacts' / role
    info = json.loads((root / 'run.json').read_text())
    assert info['databaseId'] == frozen['correctness_runs'][role]
    assert info['headSha'] == frozen[role]['commit']
    assert info['status'] == 'completed' and info['conclusion'] == 'success'
    artifact = root / 'artifact'
    prefix = 'benches/results/' + result_name + '/'
    # Bind all executed scripts and input manifests to their frozen Git blobs.
    for name in ['input.json', 'test.py', 'check-mutants.py', 'qualify.py', 'frozen-inputs.json']:
        assert (artifact / name).read_bytes() == blob(info['headSha'], prefix + name), (role, name)
    manifest = json.loads((artifact / 'input.json').read_text())
    assert manifest['source_sha256'] == frozen[role]['source_sha256']
    for section in ['source_sha256', 'tests_sha256']:
        for name, digest in manifest[section].items():
            assert sha(blob(info['headSha'], name)) == digest, (role, name)
    test = json.loads((artifact / 'test-status.json').read_text())
    assert test['source_sha256'] == manifest['source_sha256'] and test['tests_sha256'] == manifest['tests_sha256']
    assert test['exit_code'] == 0 and test['release_passed'] == count and test['release_ignored'] == 2
    assert not test['performance_qualified']
    assert 'release: 1.96.0' in test['compiler'] and 'host: x86_64-unknown-linux-gnu' in test['compiler']
    log = (artifact / 'full-release.log').read_text()
    counts = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
    assert sum(int(c[0]) for c in counts) == count and sum(int(c[2]) for c in counts) == 2
    assert all(int(c[1]) == 0 for c in counts)
    # Re-evaluate the named-test assertions from the exact frozen script without running Cargo.
    import ast
    tree = ast.parse((artifact / 'test.py').read_text())
    named = [n for n in tree.body if isinstance(n, ast.For) and isinstance(n.target, ast.Name) and n.target.id == 'name']
    assert len(named) == 1
    names = ast.literal_eval(named[0].iter)
    assert len(names) == (4 if role == 'baseline' else 6)
    for name in names:
        assert 'test ' + name + ' ... ok' in log, name
    qualification = json.loads((artifact / 'qualification-status.json').read_text())
    assert qualification['source_commit'] == info['headSha']
    assert qualification['test.py'] == qualification['check-mutants.py'] == 0
    assert qualification['sources_restored'] and qualification['tests_unchanged']
    assert not qualification['performance_qualified']
    mutant = json.loads((artifact / 'mutant-status.json').read_text())
    assert mutant.get('sources_restored', mutant.get('source_restored')) is True
    assert len(mutant['checks']) == mutations
    exec_source = blob(info['headSha'], 'src/exec.rs').decode()
    store_source = blob(info['headSha'], 'src/store.rs').decode()
    common = {
        'lost-prefix-journal': ('src/exec.rs', 'let prefix = std::mem::replace(&mut self.undo, Undo::None);', 'let _discarded = std::mem::replace(&mut self.undo, Undo::None); let prefix = Undo::None;', 1),
        'missing-checkpoint': ('src/exec.rs', 'self.checkpoint_undo();', '();', 2),
        'reversed-checkpoint-order': ('src/exec.rs', 'self.store.mem_restore(snapshot);\n                self.rollback(prefix, None);', 'self.rollback(prefix, None);\n                self.store.mem_restore(snapshot);', 1),
    }
    if role == 'candidate':
        common.update({
            'guard-derived-hash': ('src/exec.rs', 'if k == "hash" && !explicit_hash {', 'if false {', 1),
            'skip-explicit-hash-guard': ('src/exec.rs', 'if k == "hash" && !explicit_hash {', 'if k == "hash" {', 1),
        })
    else:
        a = store_source.index('        // Appended vector nodes must not outlive')
        b = store_source.index('        self.next_id = mark.next_id;', a)
        common.update({
            'missing-delete-vector': ('src/store.rs', '            self.hnsw_insert_slab(collection, p, std::slice::from_ref(doomed));\n', '', 1),
            'stale-append-vector': ('src/store.rs', store_source[a:b], '', 1),
            'missing-ordered-append': ('src/exec.rs', 'events.push(RowUndo::Append(self.db.store.append_mark()));', '();', 1),
        })
    assert {c['label'] for c in mutant['checks']} == set(common)
    for check in mutant['checks']:
        label = check['label']; name, old, new, occurrences = common[label]
        source = exec_source if name == 'src/exec.rs' else store_source
        assert source.count(old) == occurrences
        expected = source.replace(old, new).encode()
        assert (artifact / (label + '-mutant.rs')).read_bytes() == expected
        assert check['mutant_sha256'] == sha(expected)
        assert check['compiled_runtime_rejected'] and check['exit_code'] == 101
        runtime = (artifact / (label + '-runtime.log')).read_text()
        assert 'Finished `release` profile' in runtime and 'running 1 test' in runtime
        assert 'test result: FAILED. 0 passed; 1 failed;' in runtime
        assert re.search(r'test [^\n]*' + re.escape(check['test']) + r' \.\.\. FAILED', runtime)
    if role == 'baseline':
        assert qualification['point_value_checks'] == 6 and qualification['point_value_oracle_negatives'] == 5
        oracle = [json.loads(s) for s in (artifact / 'point-values-oracle.log').read_text().splitlines() if s.startswith('{')]
        assert oracle == [dict(full_value_checks=6, negative_oracle_checks=5, performance_qualified=False)]
    results[role] = dict(run_id=info['databaseId'], source_commit=info['headSha'], raw_artifact_verified=True,
                         qualification_passed=True, release_passed=count, release_ignored=2,
                         compiled_negative_checks=mutations,
                         artifact_sha256={str(f.relative_to(artifact)): sha(f.read_bytes()) for f in sorted(artifact.rglob('*')) if f.is_file()})
(p / 'correctness-prerequisites.json').write_text(json.dumps(results, indent=2) + '\n')
print('PASS: both frozen Linux correctness artifacts independently audited')
