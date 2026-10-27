from pathlib import Path
import json,hashlib,subprocess,os
p=Path(__file__).resolve().parent;m=json.loads((p/'io-provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored'];assert all(h(f)==v for f,v in m['source_sha256'].items());binary=m['binary'];assert h(binary['path'])==binary['sha256'];out=p/'io';out.mkdir();records=[];status=[]
for repeat in range(1,13):
 env=dict(os.environ,LIN_SINGLE_BUFFER_IO='1',LIN_SINGLE_BUFFER_ORDER=str(repeat));load=os.getloadavg()
 with(out/f'{repeat:02}.log').open('w')as log:
  child=subprocess.Popen([binary['path'],'--exact','persist::single_buffer_oracle::io_measurement','--nocapture'],env=env,stdout=log,stderr=subprocess.STDOUT);pid=child.pid;code=child.wait()
 text=(out/f'{repeat:02}.log').read_text();assert code==0 and 'test result: ok.'in text;rows=[dict(repeat=repeat,**json.JSONDecoder().raw_decode(part)[0])for part in text.split('SINGLE_BUFFER_IO ')[1:]];assert len(rows)==288;records.extend(rows);status.append(dict(repeat=repeat,pid=pid,exit_code=code,load_before=load,load_after=os.getloadavg()));(p/'io-records.json').write_text(json.dumps(records,indent=2)+'\n');(p/'io-status.json').write_text(json.dumps(status,indent=2)+'\n');assert h(binary['path'])==binary['sha256']and all(h(f)==v for f,v in m['source_sha256'].items())
print('Complete:12 fresh processes,3456 encode/CRC/write/sync observations with exact-frame readback')
