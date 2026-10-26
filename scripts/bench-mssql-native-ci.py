#!/usr/bin/env python3
"""Qualify the real SQL Server adapter on a dedicated native Linux x86 CI host."""
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/lin-peer-worker'))
    parser.add_argument('--image', default='mcr.microsoft.com/mssql/server:2022-latest')
    args = parser.parse_args()
    assert sys.platform == 'linux' and platform.machine() == 'x86_64', 'native Linux x86-64 required'
    args.output.mkdir(parents=True, exist_ok=False)
    sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
    capture = lambda command: subprocess.check_output(command, text=True).strip()
    tracked = capture(['git', 'ls-files', 'src', 'lin-derive', 'tools/peer-worker', 'Cargo.toml', 'Cargo.lock', 'examples/peer_bench.rs', 'scripts/bench-peers.py', 'scripts/bench-mssql-native-ci.py']).splitlines()
    sources = {path: sha(path) for path in tracked}
    owner = 'lin-mssql-native-ci-' + uuid.uuid4().hex
    database = 'lin_ci_' + uuid.uuid4().hex
    password = 'LinCi1!' + secrets.token_urlsafe(24)
    if os.environ.get('GITHUB_ACTIONS') == 'true':
        print('::add-mask::' + password, flush=True)
    env = dict(os.environ, MSSQL_SA_PASSWORD=password, LIN_BENCH_MSSQL_HOST='127.0.0.1', LIN_BENCH_MSSQL_PORT='51433', LIN_BENCH_MSSQL_USER='sa', LIN_BENCH_MSSQL_PASSWORD=password, LIN_BENCH_MSSQL_DATABASE=database)
    meta = dict(source_commit=capture(['git', 'rev-parse', 'HEAD']), source_sha256=sources, binary_sha256=sha(args.binary), rustc=capture(['rustc', '-Vv']), host=platform.platform(), architecture=platform.machine(), status='starting', runs=[], owner=owner)
    def save():
        (args.output / 'environment.json').write_text(json.dumps(meta, indent=2) + '\n')
    cid = None
    master = None
    connection = None
    save()
    try:
        subprocess.run(['docker', 'pull', args.image], check=True)
        image = json.loads(capture(['docker', 'image', 'inspect', args.image]))[0]
        assert image['Architecture'] == 'amd64' and image['Os'] == 'linux'
        meta['image'] = {key: image.get(key) for key in ['Id', 'RepoDigests', 'Architecture', 'Os']}
        cid = subprocess.check_output(['docker', 'run', '-d', '--name', owner, '--label', 'lin.benchmark.owner=' + owner, '-p', '127.0.0.1:51433:1433', '-e', 'ACCEPT_EULA=Y', '-e', 'MSSQL_PID=Developer', '-e', 'MSSQL_SA_PASSWORD', args.image], env=env, text=True).strip()
        (args.output / 'container-id.txt').write_text(cid + '\n')
        meta['container_id'] = cid
        save()
        import pytds
        for attempt in range(120):
            try:
                master = pytds.connect(server='127.0.0.1', port=51433, user='sa', password=password, database='master', autocommit=True, login_timeout=2)
                break
            except Exception:
                state = json.loads(capture(['docker', 'inspect', cid]))[0]['State']
                assert state['Running'], 'SQL Server terminated during readiness'
                meta['readiness_attempt'] = attempt
                save()
                time.sleep(1)
        else:
            raise RuntimeError('SQL Server readiness timeout')
        with master.cursor() as cursor:
            cursor.execute("SELECT CAST(SERVERPROPERTY('ProductVersion') AS VARCHAR(128)), CAST(SERVERPROPERTY('Edition') AS VARCHAR(128)), @@VERSION")
            version = cursor.fetchone()
            meta['server'] = dict(version=version[0], edition=version[1], full_version=version[2])
            cursor.execute('CREATE DATABASE ' + database)
        connection = pytds.connect(server='127.0.0.1', port=51433, user='sa', password=password, database=database, autocommit=True, login_timeout=5)
        with connection.cursor() as cursor:
            cursor.execute("SELECT name, value_in_use FROM sys.configurations WHERE name IN ('max degree of parallelism','max server memory (MB)','min server memory (MB)')")
            meta['server_configuration'] = dict(cursor.fetchall())
            cursor.execute("SELECT recovery_model_desc, delayed_durability_desc FROM sys.databases WHERE name=%s", (database,))
            row = cursor.fetchone()
            meta['database_configuration'] = dict(recovery_model=row[0], delayed_durability=row[1])
            cursor.execute("CREATE TABLE dbo.lin_ci_sentinel (marker VARCHAR(32) NOT NULL)")
            cursor.execute("INSERT INTO dbo.lin_ci_sentinel VALUES ('keep')")
        for mode, cases in [('read', []), ('native', ['--cases', 'insert_native'])]:
            for rows in [1000, 10000, 100000]:
                meta['status'] = f'measuring-{mode}-{rows}'
                save()
                out = args.output / mode / f'rows-{rows}'
                command = [sys.executable, 'scripts/bench-peers.py', '--engines', 'lin', 'mssql', *cases, '--rows', str(rows), '--samples', '12', '--repeats', '3', '--require-wins', '--lin-binary', str(args.binary.resolve()), '--output', str(out)]
                log_path = args.output / f'{mode}-{rows}.log'
                with log_path.open('w') as log:
                    result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
                record = dict(mode=mode, rows=rows, exit_code=result.returncode)
                meta['runs'].append(record)
                with connection.cursor() as cursor:
                    cursor.execute('SELECT marker FROM dbo.lin_ci_sentinel')
                    assert cursor.fetchall() == [('keep',)]
                    cursor.execute("SELECT name FROM sys.tables WHERE name LIKE 'linbench[_]%'")
                    left = [row[0] for row in cursor.fetchall()]
                    record['sentinel_unchanged'] = True
                    record['fixture_objects_remaining'] = left
                    assert not left
                save()
                assert result.returncode in (0, 2, 3), 'benchmark worker failed before normal report'
                print(mode, rows, 'exit', result.returncode, flush=True)
        assert all(sha(path) == digest for path, digest in sources.items())
        assert sha(args.binary) == meta['binary_sha256']
        meta['source_unchanged'] = True
        meta['status'] = 'finished'
        save()
    except Exception as error:
        meta['status'] = 'failed'
        meta['error_type'] = type(error).__name__
        save()
        raise RuntimeError('SQL Server qualification failed; inspect saved status/logs') from None
    finally:
        if connection is not None:
            connection.close()
        if master is not None:
            master.close()
        if cid is not None:
            info = json.loads(capture(['docker', 'inspect', cid]))[0]
            assert info['Config']['Labels'].get('lin.benchmark.owner') == owner
            logs = subprocess.run(['docker', 'logs', cid], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            (args.output / 'server.log').write_text(logs.stdout.replace(password, '[redacted]'))
            stop = subprocess.run(['docker', 'stop', cid], stdout=subprocess.DEVNULL)
            remove = subprocess.run(['docker', 'rm', cid], stdout=subprocess.DEVNULL)
            meta['cleanup'] = dict(stop_exit=stop.returncode, remove_exit=remove.returncode)
            save()
            assert stop.returncode == remove.returncode == 0
    return 0 if all(run['exit_code'] == 0 for run in meta['runs']) else 2


if __name__ == '__main__':
    raise SystemExit(main())
