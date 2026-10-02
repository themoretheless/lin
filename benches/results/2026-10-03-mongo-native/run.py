from pathlib import Path
import subprocess,json,hashlib
from pymongo import MongoClient
root=Path(__file__).resolve().parent
client=MongoClient('mongodb://127.0.0.1:27027',serverSelectionTimeoutMS=5000)
name='lin_native_sentinel_20261003'
assert name not in client.list_database_names()
client[name].docs.insert_one({'_id':'keep','marker':'unchanged'})
verify=[]
try:
 for rows in [1000,10000]:
  cmd=['.bench-venv/bin/python','scripts/bench-peers.py','--engines','lin','mongo','--cases','insert_native','--rows',str(rows),'--samples','9','--repeats','3','--require-wins','--lin-binary','/tmp/lin-mongo-native-worker','--output',str(root/f'rows-{rows}')]
  with (root/f'rows-{rows}.log').open('w') as log:result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
  assert result.returncode in [0,3],result.returncode
  assert list(client[name].docs.find({}))==[{'_id':'keep','marker':'unchanged'}]
  remaining=[n for n in client.list_database_names() if n.startswith('linbench_')]
  assert not remaining,remaining
  verify.append({'rows':rows,'exit_code':result.returncode,'sentinel_unchanged':True,'fixture_databases_remaining':remaining})
  print(rows,result.returncode,flush=True)
 (root/'verification.json').write_text(json.dumps({'runs':verify,'version':client.server_info()['version'],'binary_sha256':hashlib.sha256(Path('/tmp/lin-mongo-native-worker').read_bytes()).hexdigest()},indent=2))
finally:
 client.drop_database(name);client.close()
 subprocess.run(['docker','stop','lin-native-mongo-20261003'],check=True,stdout=subprocess.DEVNULL)
