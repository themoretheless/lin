from pathlib import Path
import subprocess,json,hashlib,time,sys,os,shutil
import psycopg,pymysql
from pymongo import MongoClient
r=Path(__file__).resolve().parent;owner='persistent-peer-refresh-20261026';created=[]
def capture(cmd):return subprocess.check_output(cmd,text=True).strip()
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
source={str(f):sha(f) for f in Path('src').rglob('*.rs')}
source.update({str(f):sha(f) for f in [Path('Cargo.toml'),Path('Cargo.lock'),Path('examples/peer_bench.rs'),Path('scripts/bench-peers.py')]})
shutil.copy2('target/release/examples/peer_bench','/tmp/lin-persistent-peer-current');shutil.copy2('/tmp/lin-peer-refresh-build-20261026.log',r/'build.log')
metadata={'source_commit':capture(['git','rev-parse','HEAD']),'source_sha256':source,'binary_sha256':sha('/tmp/lin-persistent-peer-current'),'rustc':capture(['rustc','-Vv']),'status':'starting','images':{},'containers':{},'measurement_status':{},'missing_endpoint_presence':{k:bool(os.environ.get(k)) for k in ['LIN_BENCH_MSSQL_HOST','LIN_BENCH_MSSQL_USER','LIN_BENCH_MSSQL_PASSWORD','LIN_BENCH_MSSQL_DATABASE','LIN_BENCH_KUSTO_URL','LIN_BENCH_KUSTO_DATABASE','LIN_BENCH_KUSTO_TOKEN']}}
def save():(r/'environment.json').write_text(json.dumps(metadata,indent=2)+'\n')
os.environ.update(LIN_BENCH_PG_URL='postgresql://lin:lin@127.0.0.1:55433/lin',LIN_BENCH_MYSQL_URL='mysql://lin:lin@127.0.0.1:53307/lin',LIN_BENCH_MONGO_URL='mongodb://127.0.0.1:27028')
specs=[('lin-wal-peer-pg-20261026','postgres:16-alpine',['-p','127.0.0.1:55433:5432','--tmpfs','/var/lib/postgresql/data:rw,size=2g','-e','POSTGRES_USER=lin','-e','POSTGRES_PASSWORD=lin','-e','POSTGRES_DB=lin'],[]),('lin-wal-peer-mysql-20261026','mysql:8.4',['-p','127.0.0.1:53307:3306','--tmpfs','/var/lib/mysql:rw,size=2g','-e','MYSQL_ROOT_PASSWORD=lin-benchmark-only','-e','MYSQL_USER=lin','-e','MYSQL_PASSWORD=lin','-e','MYSQL_DATABASE=lin'],[]),('lin-wal-peer-mongo-20261026','mongo:8.0',['-p','127.0.0.1:27028:27017','--tmpfs','/data/db:rw,size=2g'],['--bind_ip_all','--wiredTigerCacheSizeGB','0.25'])]
try:
 for name,tag,opts,args in specs:
  im=json.loads(capture(['docker','image','inspect',tag]))[0];metadata['images'][tag]={k:im.get(k) for k in ['Id','RepoDigests','Architecture','Os']}
  assert subprocess.run(['docker','inspect',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode!=0,'name already exists; inspect ownership before reuse'
  cid=capture(['docker','create','--name',name,'--label','lin.benchmark.owner='+owner,*opts,tag,*args]);created.append(cid);save()
  capture(['docker','start',cid]);info=json.loads(capture(['docker','inspect',cid]))[0]
  assert info['Config']['Labels'].get('lin.benchmark.owner')==owner and info['Image']==im['Id']
  metadata['containers'][name]={'id':cid,'image':info['Image'],'tmpfs':info['HostConfig']['Tmpfs'],'ports':info['HostConfig']['PortBindings']};save()
 for engine in ['postgres','mysql','mongo']:
  last=None
  for attempt in range(120):
   metadata['readiness']={'engine':engine,'attempt':attempt};save()
   try:
    if engine=='postgres':
     c=psycopg.connect(os.environ['LIN_BENCH_PG_URL'],autocommit=True,connect_timeout=2);row=c.execute("SELECT version(), current_setting('synchronous_commit'), current_setting('fsync'), current_setting('full_page_writes')").fetchone();c.close();metadata['postgres']={'version':row[0],'synchronous_commit':row[1],'fsync':row[2],'full_page_writes':row[3]}
    elif engine=='mysql':
     c=pymysql.connect(host='127.0.0.1',port=53307,user='lin',password='lin',database='lin',autocommit=True,connect_timeout=2)
     with c.cursor() as cur:cur.execute('SELECT VERSION(), @@innodb_flush_log_at_trx_commit, @@sync_binlog');row=cur.fetchone()
     c.close();metadata['mysql']={'version':row[0],'innodb_flush_log_at_trx_commit':row[1],'sync_binlog':row[2]}
    else:
     c=MongoClient(os.environ['LIN_BENCH_MONGO_URL'],serverSelectionTimeoutMS=1000);c.admin.command('ping');metadata['mongo']={'version':c.server_info()['version'],'options':c.admin.command('getCmdLineOpts')};c.close()
    save();print(engine,'ready',flush=True);break
   except Exception as e:last=type(e).__name__;time.sleep(1)
  else:raise RuntimeError(engine+' readiness failed: '+str(last))
 for mode in ['read','native']:
  metadata['status']='measuring-'+mode;save();print(mode,'measuring',flush=True)
  with (r/(mode+'.log')).open('w') as log:p=subprocess.run([sys.executable,str(r/mode/'measure.py')],stdout=log,stderr=subprocess.STDOUT)
  metadata['measurement_status'][mode]=p.returncode;save();assert p.returncode==0
  assert all(sha(f)==h for f,h in source.items());assert sha('/tmp/lin-persistent-peer-current')==metadata['binary_sha256']
 metadata['status']='finished';metadata['source_unchanged']=True;save()
except Exception as e:
 metadata['status']='failed';metadata['error_type']=type(e).__name__;save();raise
finally:
 metadata['cleanup']={}
 for cid in reversed(created):
  info=json.loads(capture(['docker','inspect',cid]))[0];assert info['Config']['Labels'].get('lin.benchmark.owner')==owner
  stop=subprocess.run(['docker','stop',cid],stdout=subprocess.DEVNULL);remove=subprocess.run(['docker','rm',cid],stdout=subprocess.DEVNULL)
  metadata['cleanup'][cid]={'stop_exit':stop.returncode,'remove_exit':remove.returncode};save();assert stop.returncode==remove.returncode==0
