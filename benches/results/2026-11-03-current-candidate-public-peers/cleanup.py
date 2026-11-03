from pathlib import Path
import json,subprocess
p=Path('/tmp/lin-current-candidate-public-peers-20261103');f=p/'environment.json'
if f.exists():
 m=json.loads(f.read_text());checks=[]
 for cid in m.get('created_container_ids',[]):
  probe=subprocess.run(['docker','inspect',cid],capture_output=True,text=True)
  if probe.returncode==0:
   info=json.loads(probe.stdout)[0];assert info['Config']['Labels'].get('lin.benchmark.owner')=='current-candidate-peers-20261103'
   subprocess.run(['docker','stop',cid],stdout=subprocess.DEVNULL);subprocess.run(['docker','rm',cid],stdout=subprocess.DEVNULL)
  absent=subprocess.run(['docker','inspect',cid],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode==1;checks.append({'container_id':cid,'absent':absent})
 (p/'cleanup-proof.json').write_text(json.dumps({'all_owned_containers_absent':all(c['absent']for c in checks),'checks':checks},indent=2)+'\n');assert all(c['absent']for c in checks)
