from pathlib import Path
import importlib.util,uuid,json
from types import SimpleNamespace
import psycopg
root=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('bench',Path('scripts/bench-peers.py'))
bench=importlib.util.module_from_spec(spec);spec.loader.exec_module(bench)
prefix='linbench_'+uuid.uuid4().hex[:16]
conn=psycopg.connect('postgresql://lin:lin@127.0.0.1:55432/lin',autocommit=True)
sentinel='linbench_sentinel_'+uuid.uuid4().hex[:16]
from psycopg import sql
name=sql.Identifier(sentinel)
conn.execute(sql.SQL('CREATE TABLE {} (v TEXT)').format(name))
conn.execute(sql.SQL('INSERT INTO {} VALUES (%s)').format(name),['unchanged'])
peer=None
try:
 data=bench.dataset(41)
 for i,text in enumerate(['', 'comma,quote"', 'line\nbreak', 'Unicode Привет Σ']):data[0][i]=[*data[0][i][:4],text]
 peer=bench.NativeInsert(data,SimpleNamespace(native_timestamp_ms=123456),prefix,'postgres')
 timings,iterations,rows=peer.measure_insert(3)
 bench.validate(rows,peer.want,'postgres','insert_native')
 assert len(timings)==3 and iterations==1
 peer.want[1][1]=peer.want[0][1]
 import csv,io
 stream=io.StringIO(newline='');csv.writer(stream,quoting=csv.QUOTE_ALL).writerows(peer.want)
 peer.copy_bytes=stream.getvalue().encode('utf-8')
 try:peer.measure_insert(1)
 except psycopg.errors.UniqueViolation:pass
 else:raise AssertionError('duplicate URI was accepted')
 assert conn.execute(sql.SQL('SELECT v FROM {}').format(name)).fetchall()==[('unchanged',)]
 remaining=conn.execute("SELECT tablename FROM pg_tables WHERE schemaname='public' AND tablename LIKE %s",[prefix+'%']).fetchall()
 assert not remaining,remaining
 (root/'verification.json').write_text(json.dumps({'csv_edge_values_validated':True,'duplicate_uri_rejected':True,'sentinel_unchanged':True,'owned_tables_remaining':remaining},indent=2))
 print('CSV values, duplicate rejection, owned cleanup and sentinel checks passed')
finally:
 if peer is not None:peer.close()
 conn.execute(sql.SQL('DROP TABLE {}').format(name));conn.close()
