from pathlib import Path
import json,hashlib,statistics as st
p=Path(__file__).resolve().parent;root=p/'measurement';records=json.loads((root/'process-status.json').read_text());assert len(records)==216
cases=[(s,o)for s in [117,4095,4096,4097,4125,8192]for o in [0,2048,4095]];expected=[]
for repeat in range(1,7):
 for i,(size,offset)in enumerate(cases[repeat:]+cases[:repeat]):
  roles=['original','control']if(repeat+i)%2==0 else['control','original']
  expected.extend((repeat,size,offset,role)for role in roles)
assert [(r['round'],r['size'],r['offset'],r['role'])for r in records]==expected
med={};count=0
for r in records:
 n=f"{r['round']}-{r['size']}-{r['offset']}-{r['role']}";raw=root/(n+'.jsonl');assert r['exit_code']==0 and hashlib.sha256(raw.read_bytes()).hexdigest()==r['raw_sha256']
 rows=[json.loads(line)for line in raw.read_text().splitlines()];assert len(rows)==8
 for i,x in enumerate(rows):
  size=r['size'];offset=r['offset'];assert x['sample']==i and x['size']==size and x['offset']==offset and x['validated_bytes']==size+offset
  assert all(isinstance(x[k],int)and x[k]>0 for k in ['write_ns','sync_ns','total_ns']) and x['total_ns']>=x['write_ns']+x['sync_ns']
  data=(root/n/f'sample-{i}.wal').read_bytes();assert data==bytes([0x35])*offset+bytes([0xa7])*size;count+=1
 med[r['round'],r['size'],r['offset'],r['role']]=st.median(x['total_ns']for x in rows)
assert count==1728
summary=[]
for size,offset in cases:
 values=[med[i,size,offset,'original']for i in range(1,7)];aa=[abs(100*(med[i,size,offset,'control']/med[i,size,offset,'original']-1))for i in range(1,7)]
 summary.append(dict(size=size,offset=offset,median_total_ns=st.median(values),aa_absolute_percent=st.median(aa)))
(root/'summary.json').write_text(json.dumps(dict(processes=216,observations=count,cases=summary,source_candidate_qualified=False,full_objective_achieved=False),indent=2)+'\n')
print('216 processes / 1728 observations and all stored bytes verified')
