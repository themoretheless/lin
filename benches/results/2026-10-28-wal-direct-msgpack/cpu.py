from pathlib import Path
import json,re,shutil,hashlib,subprocess,os,platform
p=Path(__file__).resolve().parent;h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();m=json.loads((p/'test-status.json').read_text());assert m['exit_code']==0 and m['source_restored']and all(h(f)==v for f,v in m['source_sha256'].items())
source=re.search(r'Running unittests src/lib.rs \(([^)]+)\)',(p/'tests.log').read_text())[1];binary=Path('/tmp/lin-wal-direct-msgpack-tests');shutil.copy2(source,binary);meta=dict(binary=dict(path=str(binary),sha256=h(binary)),source_sha256=m['source_sha256'],helper_sha256=h(p/'helper.rs'),oracle_sha256=h(p/'oracle.rs'),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),performance_qualified=False);(p/'cpu-provenance.json').write_text(json.dumps(meta,indent=2)+'\n');out=p/'cpu';out.mkdir(exist_ok=True);status=[];records=[]
for repeat in range(1,13):
 env=dict(os.environ,LIN_WAL_ZERO_CPU='1',LIN_WAL_ZERO_ORDER=str(repeat));load=os.getloadavg()
 with(out/f'{repeat:02}.log').open('w')as log:
  child=subprocess.Popen([str(binary),'--exact','persist::wal_direct_zero_measurement::serialization_cpu_probe','--nocapture'],env=env,stdout=log,stderr=subprocess.STDOUT);pid=child.pid;code=child.wait()
 text=(out/f'{repeat:02}.log').read_text();assert code==0 and 'test result: ok.'in text
 rows=[dict(repeat=repeat,**json.JSONDecoder().raw_decode(part)[0])for part in text.split('ZERO_CPU_PROBE ')[1:]];assert len(rows)==27
 records.extend(rows);status.append(dict(repeat=repeat,pid=pid,exit_code=code,load_before=load,load_after=os.getloadavg()));(p/'cpu-records.json').write_text(json.dumps(records,indent=2)+'\n');(p/'cpu-status.json').write_text(json.dumps(status,indent=2)+'\n')
 assert h(binary)==meta['binary']['sha256']and all(h(f)==v for f,v in m['source_sha256'].items())
assert len(records)==324;print('Complete:12 fresh processes,324 encode blocks,1296000 encodes;no file I/O performance claim')
