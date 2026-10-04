from pathlib import Path
import subprocess,json,hashlib
import psycopg,pymysql
from pymongo import MongoClient
root=Path(__file__).resolve().parent
pg=psycopg.connect('postgresql://lin:lin@127.0.0.1:55432/lin',autocommit=True)
my=pymysql.connect(host='127.0.0.1',port=53306,user='lin',password='lin',database='lin',autocommit=True)
mo=MongoClient('mongodb://127.0.0.1:27027',serverSelectionTimeoutMS=5000)
name='lin_read_sentinel_20261004'
created=[];verify=[]
try:
 assert pg.execute('SELECT to_regclass(%s)',(name,)).fetchone()[0] is None
 with my.cursor() as c:
  c.execute('SHOW TABLES');assert name not in [x[0] for x in c.fetchall()]
 assert name not in mo.list_database_names()
 pg.execute('CREATE TABLE '+name+' (marker TEXT)');created.append('pg');pg.execute('INSERT INTO '+name+" VALUES ('keep')")
 with my.cursor() as c:c.execute('CREATE TABLE '+name+' (marker TEXT)');created.append('my');c.execute('INSERT INTO '+name+" VALUES ('keep')")
 mo[name].docs.insert_one({'_id':'keep','marker':'unchanged'});created.append('mo')
 for rows in [1000,10000,100000]:
  cmd=['.bench-venv/bin/python','scripts/bench-peers.py','--rows',str(rows),'--samples','8','--repeats','3','--require-wins','--lin-binary','/tmp/lin-current-peer-read-matrix','--output',str(root/f'rows-{rows}')]
  with (root/f'rows-{rows}.log').open('w') as log:result=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
  assert result.returncode in [0,2,3],result.returncode
  assert pg.execute('SELECT marker FROM '+name).fetchall()==[('keep',)]
  with my.cursor() as c:
   c.execute('SELECT marker FROM '+name);assert c.fetchall()==(('keep',),)
   c.execute('SHOW TABLES');my_left=[x[0] for x in c.fetchall() if x[0].startswith('linbench_')]
  assert list(mo[name].docs.find({}))==[{'_id':'keep','marker':'unchanged'}]
  pg_left=[x[0] for x in pg.execute("SELECT tablename FROM pg_tables WHERE schemaname='public'").fetchall() if x[0].startswith('linbench_')]
  mo_left=[x for x in mo.list_database_names() if x.startswith('linbench_')]
  assert not (pg_left or my_left or mo_left),(pg_left,my_left,mo_left)
  verify.append({'rows':rows,'exit_code':result.returncode,'sentinels_unchanged':True,'fixture_objects_remaining':{'postgres':pg_left,'mysql':my_left,'mongo':mo_left}})
  (root/'verification.json').write_text(json.dumps({'runs':verify,'binary_sha256':hashlib.sha256(Path('/tmp/lin-current-peer-read-matrix').read_bytes()).hexdigest()},indent=2))
  print(rows,result.returncode,flush=True)
finally:
 if 'pg' in created:pg.execute('DROP TABLE '+name)
 if 'my' in created:
  with my.cursor() as c:c.execute('DROP TABLE '+name)
 if 'mo' in created:mo.drop_database(name)
 pg.close();my.close();mo.close()
 subprocess.run(['docker','stop','lin-bench-postgres-1','lin-bench-mysql-1','lin-native-mongo-20261003'],check=True,stdout=subprocess.DEVNULL)
