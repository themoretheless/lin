from pathlib import Path
import hashlib,json,platform,subprocess,tempfile,time
p=Path(__file__).resolve().parent;m=json.loads((p/'provenance.json').read_text());h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest();assert m['source_restored']and all(h(f)==v for f,v in m['source_sha256'].items());assert h(m['binary']['path'])==m['binary']['sha256'];records=[]
(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),machine=platform.machine(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),diagnostic_only=True,paired_candidate_qualification=False,policy_changed=False),indent=2)+'\n')
for repeat in range(1,4):
 engines=['lin','sqlite']if repeat%2 else['sqlite','lin']
 for engine in engines:
  with tempfile.TemporaryDirectory(prefix='lin-normal-checkpoint-')as temporary:
   root=Path(temporary)/engine;started=time.time();output=p/f'{repeat:02}-{engine}.json';log=p/f'{repeat:02}-{engine}.log'
   with output.open('w')as stdout,log.open('w')as stderr:
    child=subprocess.Popen([m['binary']['path'],engine,str(root)],stdout=stdout,stderr=stderr);pid=child.pid;code=child.wait()
   record=dict(repeat=repeat,engine=engine,pid=pid,exit_code=code,elapsed_seconds=time.time()-started,output=output.name,binary_sha256=h(m['binary']['path']));records.append(record);(p/'process-status.json').write_text(json.dumps(records,indent=2)+'\n');assert code==0,(engine,log)
   d=json.loads(output.read_text());assert d['engine']==engine and d['full_values_after_reopen']and len(d['samples'])==1000
 assert all(h(f)==v for f,v in m['source_sha256'].items())
assert len(records)==6 and h(m['binary']['path'])==m['binary']['sha256'];print('PASS: six fresh processes /6000 transaction observations; full input-value reopen checks; no policy or source changes')
