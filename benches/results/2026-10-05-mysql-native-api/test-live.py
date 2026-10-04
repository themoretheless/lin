from pathlib import Path
import importlib.util,uuid,json
from types import SimpleNamespace
import pymysql
spec=importlib.util.spec_from_file_location('bench',Path('scripts/bench-peers.py'));bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
root=Path(__file__).resolve().parent
prefix='linbench_'+uuid.uuid4().hex[:16]
data=bench.dataset(41)
for i,text in enumerate(['', 'comma,quote"', 'line\nbreak', 'Unicode Привет Σ', 'back\\slash']):data[0][i]=[*data[0][i][:4],text]
peer=bench.NativeInsert(data,SimpleNamespace(native_timestamp_ms=123456),prefix,'mysql')
sentinel='linbench_sentinel_'+uuid.uuid4().hex[:16]
created=False
try:
 with peer.mysql_conn.cursor() as cursor:
  cursor.execute(f'CREATE TABLE `{sentinel}` (v TEXT)');created=True
  cursor.execute(f'INSERT INTO `{sentinel}` VALUES (%s)',['unchanged'])
 timings,iterations,rows=peer.measure_insert(3)
 bench.validate(rows,peer.want,'mysql','insert_native')
 assert len(timings)==3 and iterations==1
 peer.want[1][1]=peer.want[0][1]
 try:peer.measure_insert(1)
 except pymysql.err.IntegrityError as e:assert e.args[0]==1062
 else:raise AssertionError('duplicate URI accepted')
 with peer.mysql_conn.cursor() as cursor:
  cursor.execute(f'SELECT v FROM `{sentinel}`');assert cursor.fetchall()==(('unchanged',),)
  cursor.execute('SHOW TABLES');remaining=[row[0] for row in cursor.fetchall() if row[0].startswith(prefix)]
  assert not remaining,remaining
 (root/'verification.json').write_text(json.dumps({'escaped_values_validated':True,'duplicate_uri_rejected':True,'sentinel_unchanged':True,'owned_tables_remaining':remaining},indent=2))
 print('Escaped values, rollback, cleanup and sentinel checks passed')
finally:
 if created:
  with peer.mysql_conn.cursor() as cursor:cursor.execute(f'DROP TABLE `{sentinel}`')
 peer.close()
