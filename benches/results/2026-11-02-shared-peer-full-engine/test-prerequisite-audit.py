"""Synthetic auditor checks; fabricated fixtures are never production evidence."""
from pathlib import Path
import ast, hashlib, json, shutil, subprocess, tempfile
src = Path(__file__).resolve().parent
frozen = json.loads((src / 'source-inputs.json').read_text())
def blob(ref, name):
    return subprocess.check_output(['git', 'show', ref + ':' + name])
def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')
with tempfile.TemporaryDirectory(prefix='lin-prerequisite-audit-synthetic-') as tmp:
    p = Path(tmp)
    shutil.copy2(src / 'audit-prerequisites.py', p / 'audit-prerequisites.py')
    shutil.copy2(src / 'source-inputs.json', p / 'source-inputs.json')
    for role, count, folder in [('baseline',441,'2026-11-01-btreemap-correctness'),('candidate',458,'2026-11-01-corrected-update-patch')]:
        root = p / 'correctness-artifacts' / role; artifact = root / 'artifact'; artifact.mkdir(parents=True)
        ref = frozen[role]['commit']
        write(root / 'run.json', dict(databaseId=frozen['correctness_runs'][role],headSha=ref,status='completed',conclusion='success'))
        for n in ['input.json','test.py','check-mutants.py','qualify.py','frozen-inputs.json']:
            (artifact/n).write_bytes(blob(ref,'benches/results/'+folder+'/'+n))
        m=json.loads((artifact/'input.json').read_text())
        write(artifact/'test-status.json',dict(exit_code=0,release_passed=count,release_ignored=2,source_sha256=m['source_sha256'],tests_sha256=m['tests_sha256'],compiler='release: 1.96.0\nhost: x86_64-unknown-linux-gnu\n',performance_qualified=False))
        tree=ast.parse((artifact/'test.py').read_text());loop=next(n for n in tree.body if isinstance(n,ast.For)and isinstance(n.target,ast.Name)and n.target.id=='name');names=ast.literal_eval(loop.iter)
        (artifact/'full-release.log').write_text(''.join('test '+n+' ... ok\n'for n in names)+f'test result: ok. {count} passed; 0 failed; 2 ignored\n')
        q=dict(source_commit=ref,performance_qualified=False,sources_restored=True,tests_unchanged=True,**{'test.py':0,'check-mutants.py':0})
        if role=='baseline':
            q.update(point_value_checks=6,point_value_oracle_negatives=5)
            (artifact/'point-values-oracle.log').write_text(json.dumps(dict(full_value_checks=6,negative_oracle_checks=5,performance_qualified=False))+'\n')
        write(artifact/'qualification-status.json',q)
        store=blob(ref,'src/store.rs').decode();a=store.index('        // Appended vector nodes must not outlive');b=store.index('        self.next_id = mark.next_id;',a)
        tree=ast.parse((artifact/'check-mutants.py').read_text());node=next(n for n in tree.body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='cases'for t in n.targets))
        cases=eval(compile(ast.Expression(node.value),'<frozen mutation definitions>','eval'),{'__builtins__':{}},{'store':store,'a':a,'b':b})
        checks=[]
        for case in cases:
            if role=='candidate':label,old,new,occurrences,target,test=case;name='src/exec.rs'
            else:label,name,old,new,occurrences,args,test=case
            source=blob(ref,name).decode();assert source.count(old)==occurrences;mutant=source.replace(old,new).encode()
            (artifact/(label+'-mutant.rs')).write_bytes(mutant)
            (artifact/(label+'-runtime.log')).write_text('Finished `release` profile\nrunning 1 test\ntest '+test+' ... FAILED\ntest result: FAILED. 0 passed; 1 failed;\n')
            checks.append(dict(label=label,test=test,exit_code=101,compiled_runtime_rejected=True,mutant_sha256=hashlib.sha256(mutant).hexdigest()))
        write(artifact/'mutant-status.json',dict(checks=checks,sources_restored=True))
    def run(ok):
        r=subprocess.run(['python3',str(p/'audit-prerequisites.py')],capture_output=True,text=True)
        assert (r.returncode==0)==ok,(r.stdout,r.stderr)
    run(True)
    cases=[('wrong_commit','baseline','run.json',lambda d:d.update(headSha='0'*40)),('failed_job','candidate','run.json',lambda d:d.update(conclusion='failure')),('wrong_count','candidate','artifact/test-status.json',lambda d:d.update(release_passed=452)),('missing_mutation','baseline','artifact/mutant-status.json',lambda d:d['checks'].pop()),('source_not_restored','candidate','artifact/qualification-status.json',lambda d:d.update(sources_restored=False))]
    for label,role,name,change in cases:
        f=p/'correctness-artifacts'/role/name;original=f.read_bytes();d=json.loads(original);change(d);write(f,d);run(False);f.write_bytes(original)
    for label,role,name in [('missing_named_test','candidate','full-release.log'),('wrong_mutant_source','baseline','missing-delete-vector-mutant.rs'),('missing_compilation','candidate','lost-prefix-journal-runtime.log'),('changed_script','candidate','test.py')]:
        f=p/'correctness-artifacts'/role/'artifact'/name;original=f.read_bytes();f.write_bytes(b'corrupt fixture\n');run(False);f.write_bytes(original)
    run(True)
    print('PASS: synthetic complete artifacts accepted; nine provenance/log/mutation corruptions rejected; no real CI acceptance claimed')
