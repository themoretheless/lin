from pathlib import Path
import subprocess,json,hashlib,shutil,os
p=Path(__file__).resolve().parent
root=Path(os.environ.get('LIN_DICT_CRASH_OUT',str(p/'crash-fixtures')));reuse=bool(os.environ.get('LIN_DICT_RESUME_CREATE_FIXTURES'))
if reuse:assert root.exists() and {d.name for d in root.iterdir()}=={'ack3','ack4'}
else:assert not root.exists();root.mkdir()
worker='/tmp/lin-dictionary-crash-worker'
def digest(d):return {str(f.relative_to(d)):hashlib.sha256(f.read_bytes()).hexdigest() for f in d.rglob('*') if f.is_file()}
def execute(name,args,ok=True):
 with (p/(name+'.log')).open('w') as log:r=subprocess.run(args,stdout=log,stderr=subprocess.STDOUT)
 assert (r.returncode==0) if ok else (r.returncode!=0),(name,r.returncode)
 return r.returncode
def frames(data):
 result=[];pos=0
 while pos<len(data):
  assert data[pos:pos+4]==b'LIN\x06';n=int.from_bytes(data[pos+4:pos+8],'little');end=pos+8+n;assert end<=len(data)
  result.append({'start':pos,'end':end,'codec':data[pos+8]});pos=end
 return result
ack3=root/'ack3';ack4=root/'ack4'
if not reuse:
 execute('crash-create-3',[worker,'create',str(ack3),'3']);execute('crash-create-4',[worker,'create',str(ack4),'4'])
assert (p/'crash-create-3.log').read_text().splitlines()==['ACK 1000','ACK 2000','ACK 3000']
a=(ack3/'log').read_bytes();b=(ack4/'log').read_bytes();fa=frames(a);fb=frames(b)
data3=[f for f in fa if f['codec']==5];data4=[f for f in fb if f['codec']==5]
assert len(data3)==3 and len(data4)==4 and all(f['codec'] in [1,5] for f in fa+fb)
assert b[:data4[3]['start']]==a
before=digest(ack3);old='/tmp/lin-old-reader-before-dictionary'
old_meta=json.loads(Path('benches/results/2026-10-24-dictionary-wal-reader/old-reader-provenance.json').read_text());assert hashlib.sha256(Path(old).read_bytes()).hexdigest()==old_meta['binary_sha256']
old_exit=execute('old-reader-real-writer',[old,str(ack3)],ok=False);assert old_exit==2 and digest(ack3)==before
execute('crash-verify-3',[worker,'verify',str(ack3),'3']);assert digest(ack3)==before
execute('crash-verify-4',[worker,'verify',str(ack4),'4'])
tail=b[data4[3]['start']:];cuts=sorted(set([0,1,3,4,7,8,9,len(tail)//2,len(tail)-4,len(tail)-1]));checks=[]
for cut in cuts:
 dest=root/f'torn-{cut}';shutil.copytree(ack3,dest);(dest/'log').write_bytes(a+tail[:cut]);state=digest(dest)
 execute(f'torn-{cut}',[worker,'verify',str(dest),'3']);assert digest(dest)==state
 checks.append({'cut':cut,'acknowledged_rows':3000,'verified_rows':3000,'read_only_files_unchanged':True})
corrupt=root/'complete-corrupt';shutil.copytree(ack3,corrupt);bad=bytearray(tail);bad[len(bad)//2]^=1;(corrupt/'log').write_bytes(a+bad);state=digest(corrupt)
exit_code=execute('complete-corrupt',[worker,'verify',str(corrupt),'3'],ok=False);assert digest(corrupt)==state
(p/'crash-verification.json').write_text(json.dumps({'create_exit_bypasses_drop':True,'physical_power_loss_test':False,'acknowledged_commits':3,'acknowledged_rows':3000,'ack3_frames':fa,'ack4_frames':fb,'old_reader_exit':old_exit,'old_reader_files_unchanged':True,'tail_origin':'4th frame from separate complete deterministic timeline, appended as unacknowledged prefix to 3-ACK fixture','torn_tails':checks,'complete_corruption_exit':exit_code,'complete_corruption_files_unchanged':True,'ack3_sha256':before},indent=2)+'\n')
print('process-exit ACK recovery, real codec-5 old-reader rejection, torn tails and corruption passed',flush=True)
