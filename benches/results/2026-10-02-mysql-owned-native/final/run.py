import os,subprocess,pathlib,json,time,hashlib
import pymysql
root=pathlib.Path(__file__).resolve().parent
name='linbench_validate_'+str(os.getpid())
def admin(sql):
 subprocess.run(['docker','exec','-i','-e','MYSQL_PWD=lin','lin-bench-mysql-1','mysql','-uroot'],input=sql,text=True,check=True,stdout=subprocess.DEVNULL)
binary='/tmp/lin-mysql-owned-native'
conn=None
try:
 admin(f"CREATE DATABASE `{name}`; GRANT ALL PRIVILEGES ON `{name}`.* TO 'lin'@'%';")
 conn=pymysql.connect(host='127.0.0.1',port=53306,user='lin',password='lin',database=name,autocommit=True)
 tables=['docs','docs_bulk','users','orders','logs_bulk']
 with conn.cursor() as c:
  for table in tables:
   c.execute(f"CREATE TABLE `{table}` (marker VARCHAR(80) PRIMARY KEY)")
   c.execute(f"INSERT INTO `{table}` VALUES ('keep-{table}')")
  c.execute("SELECT VERSION(), @@innodb_flush_log_at_trx_commit, @@sync_binlog, @@max_allowed_packet")
  settings=c.fetchone()
 env=dict(os.environ,LIN_BENCH_MYSQL_URL=f'mysql://lin:lin@127.0.0.1:53306/{name}')
 env.pop('LIN_BENCH_SKIP_MYSQL',None);env.pop('LIN_BENCH_MIXED_CASE',None)
 runs=[('safety',['--filter','*/mysql','--glob','--samples','1'])]
 runs += [(f'run-{i}',['--filter','compare/insert_*/*','--glob','--exclude','*insert_phase*','--exclude','*/duckdb','--exclude','*/mysql','--exclude','*/postgres*','--samples','8']) for i in range(1,4)]
 verification=[]
 for label,selection in runs:
  cmd=[binary,*selection,'--max-iterations','1','--warmup-ms','0','--sample-ms','1','--output',str(root/label)]
  with (root/(label+'.log')).open('w') as log:subprocess.run(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
  with conn.cursor() as c:
   for table in tables:
    c.execute(f"SELECT marker FROM `{table}`");assert c.fetchall()==(('keep-'+table,),)
   c.execute('SHOW TABLES');all_tables=[x[0] for x in c.fetchall()];assert sorted(all_tables)==sorted(tables),all_tables
  verification.append({'run':label,'sentinels_unchanged':True,'owned_tables_remaining':0})
  print(label,flush=True)
 (root/'verification.json').write_text(json.dumps({'runs':verification,'mysql_settings':settings,'binary_sha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()},indent=2))
finally:
 if conn:conn.close()
 admin(f"REVOKE ALL PRIVILEGES ON `{name}`.* FROM 'lin'@'%'; DROP DATABASE `{name}`;")
 subprocess.run(['docker','stop','lin-bench-mysql-1'],check=True,stdout=subprocess.DEVNULL)
