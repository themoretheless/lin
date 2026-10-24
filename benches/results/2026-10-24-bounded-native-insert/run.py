from pathlib import Path
import subprocess,json,hashlib,time,sys
import psycopg,pymysql
from pymongo import MongoClient
p=Path(__file__).resolve().parent
started=[]
import os
os.environ.update(LIN_BENCH_PG_URL='postgresql://lin:lin@127.0.0.1:55432/lin',LIN_BENCH_MYSQL_URL='mysql://lin:lin@127.0.0.1:53306/lin',LIN_BENCH_MONGO_URL='mongodb://127.0.0.1:27027')
metadata={'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'images':{},'containers':{},'status':'starting'}
reference=json.loads(Path('benches/results/2026-10-24-post-count-read-matrix/provenance.json').read_text())
assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in reference['source_sha256'].items())
assert hashlib.sha256(Path('/tmp/lin-bounded-native-insert').read_bytes()).hexdigest()==json.loads((p/'binary-sha256.json').read_text())['sha256']
metadata.update(binary_sha256=json.loads((p/'binary-sha256.json').read_text())['sha256'],source_sha256=reference['source_sha256'])
metadata['benchmark_source_sha256']={str(f):hashlib.sha256(f.read_bytes()).hexdigest() for f in [Path('examples/peer_bench.rs'),Path('scripts/bench-peers.py')]}
def save(): (p/'environment.json').write_text(json.dumps(metadata,indent=2)+'\n')
def capture(cmd):return subprocess.check_output(cmd,text=True).strip()
specs=[
 ('lin-read-pg-20261024','postgres:16-alpine',['-p','127.0.0.1:55432:5432','--tmpfs','/var/lib/postgresql/data:rw,size=2g','-e','POSTGRES_USER=lin','-e','POSTGRES_PASSWORD=lin','-e','POSTGRES_DB=lin'],[]),
 ('lin-read-mysql-20261024','mysql:8.4',['-p','127.0.0.1:53306:3306','--tmpfs','/var/lib/mysql:rw,size=2g','-e','MYSQL_ROOT_PASSWORD=lin-benchmark-only','-e','MYSQL_USER=lin','-e','MYSQL_PASSWORD=lin','-e','MYSQL_DATABASE=lin'],[]),
 ('lin-read-mongo-20261024','mongo:8.0',['-p','127.0.0.1:27027:27017','--tmpfs','/data/db:rw,size=2g'],['--bind_ip_all','--wiredTigerCacheSizeGB','0.25'])]
try:
 for name,tag,opts,args in specs:
  im=json.loads(capture(['docker','image','inspect',tag]))[0]
  metadata['images'][tag]={k:im.get(k) for k in ['Id','RepoDigests','Architecture','Os']}
  info=json.loads(capture(['docker','inspect',name]))[0]
  assert info['Config']['Labels'].get('lin.benchmark.owner')=='read-matrix-20261024'
  assert not info['State']['Running']
  assert info['Image']==im['Id']
  cid=info['Id']
  capture(['docker','start',name]);started.append(name)
  info=json.loads(capture(['docker','inspect',cid]))[0]
  metadata['containers'][name]={'id':cid,'image':info['Image'],'tmpfs':info['HostConfig']['Tmpfs'],'ports':info['HostConfig']['PortBindings']}
  save()
 for engine in ['postgres','mysql','mongo']:
  last=None
  for attempt in range(120):
   try:
    if engine=='postgres':
     c=psycopg.connect('postgresql://lin:lin@127.0.0.1:55432/lin',autocommit=True,connect_timeout=2)
     row=c.execute('SELECT version(), current_setting(\'synchronous_commit\'), current_setting(\'fsync\'), current_setting(\'full_page_writes\')').fetchone();c.close()
     metadata['postgres']={'version':row[0],'synchronous_commit':row[1],'fsync':row[2],'full_page_writes':row[3]}
    elif engine=='mysql':
     c=pymysql.connect(host='127.0.0.1',port=53306,user='lin',password='lin',database='lin',autocommit=True,connect_timeout=2)
     with c.cursor() as cur:cur.execute('SELECT VERSION(), @@innodb_flush_log_at_trx_commit, @@sync_binlog');row=cur.fetchone()
     c.close();metadata['mysql']={'version':row[0],'innodb_flush_log_at_trx_commit':row[1],'sync_binlog':row[2]}
    else:
     c=MongoClient('mongodb://127.0.0.1:27027',serverSelectionTimeoutMS=1000);c.admin.command('ping');metadata['mongo']={'version':c.server_info()['version'],'options':c.admin.command('getCmdLineOpts')};c.close()
    save();print(engine,'ready',flush=True);break
   except Exception as e:last=type(e).__name__;time.sleep(1)
  else:raise RuntimeError(engine+' readiness failed: '+str(last))
 metadata['status']='measuring';save()
 with (p/'measure.log').open('w') as log:r=subprocess.run([sys.executable,str(p/'measure.py')],stdout=log,stderr=subprocess.STDOUT)
 metadata['measurement_exit_code']=r.returncode
 assert r.returncode==0,r.returncode
 assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in reference['source_sha256'].items())
 assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in metadata['benchmark_source_sha256'].items())
 metadata['source_unchanged']=True;metadata['status']='finished';save()
except Exception as e:
 metadata['status']='failed';metadata['error_type']=type(e).__name__;save();raise
finally:
 for name in started:
  info=json.loads(capture(['docker','inspect',name]))[0]
  if info['State']['Running']:subprocess.run(['docker','stop',name],check=True,stdout=subprocess.DEVNULL)
 metadata['final_states']={name:json.loads(capture(['docker','inspect',name]))[0]['State']['Status'] for name in started};save()
print(metadata['status'],flush=True)
