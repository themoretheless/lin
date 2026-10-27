from pathlib import Path
import subprocess,json,hashlib,shutil,statistics as st
p=Path(__file__).resolve().parent;index=Path('src/index.rs');before=index.read_bytes();lib=Path('src/lib.rs');before_lib=lib.read_bytes();candidate=p.parent/'2026-10-27-cow-bitmap-prefix-empty'/'candidate-index.rs';example=Path('examples/bitmap_prefix_empty_cpu.rs');assert not example.exists();statuses=[];meta={'scope':'pure PostingList kernel, not database qualification','before_index_sha256':hashlib.sha256(before).hexdigest(),'candidate_index_sha256':hashlib.sha256(candidate.read_bytes()).hexdigest(),'gate_sha256':hashlib.sha256((p/'gate.rs').read_bytes()).hexdigest()}
try:
 probe=(p/'gate.rs').read_text().replace('use lin::index::PostingList;','use super::PostingList;').replace('fn main(){','pub fn run(process:usize){').replace(' let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();let mut cases=Vec::new();',' let mut cases=Vec::new();')
 index.write_bytes(candidate.read_bytes()+('\npub mod empty_cpu_gate {\n'+probe+'\n}\n').encode())
 lib.write_bytes(before_lib+b'\npub fn empty_cpu_gate(process:usize){crate::index::empty_cpu_gate::run(process); }\n')
 example.write_text('fn main(){lin::empty_cpu_gate(std::env::args().nth(1).unwrap().parse().unwrap());}\n')
 meta['probe_index_sha256']=hashlib.sha256(index.read_bytes()).hexdigest();meta['probe_lib_sha256']=hashlib.sha256(lib.read_bytes()).hexdigest()
 with(p/'build.log').open('w')as log:r=subprocess.run(['cargo','build','--release','--example','bitmap_prefix_empty_cpu'],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0
 binary=Path('/tmp/lin-bitmap-prefix-empty-cpu');shutil.copy2('target/release/examples/bitmap_prefix_empty_cpu',binary);meta['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
finally:
 index.write_bytes(before);lib.write_bytes(before_lib);example.unlink(missing_ok=True);(p/'provenance.json').write_text(json.dumps(meta,indent=2)+'\n')
for i in range(1,7):
 with(p/f'{i:02}.jsonl').open('w')as out,(p/f'{i:02}.log').open('w')as log:r=subprocess.run([str(binary),str(i)],stdout=out,stderr=log)
 statuses.append({'process':i,'exit_code':r.returncode});(p/'status.json').write_text(json.dumps(statuses,indent=2)+'\n');assert r.returncode==0
rows={}
for i in range(1,7):
 data=[json.loads(line)for line in(p/f'{i:02}.jsonl').read_text().splitlines()];assert len(data)==14
 for r in data:
  assert r['exact_membership']and len(r['samples'])==3 and all(len(s)==24 for s in r['samples']);a,aa,b=[st.median(s)for s in r['samples']];rows.setdefault(r['case'],[]).append({'process':i,'baseline_ns':a,'control_ns':aa,'candidate_ns':b,'gain_percent':100*(1-b/a),'aa_percent':100*(1-aa/a)})
summary={case:{'gain_percent':st.median(r['gain_percent']for r in group),'aa_absolute_percent':st.median(abs(r['aa_percent'])for r in group),'positive_processes':sum(r['gain_percent']>0 for r in group),'processes':group}for case,group in rows.items()}
(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for case,s in summary.items():print(case,round(s['gain_percent'],3),round(s['aa_absolute_percent'],3),s['positive_processes'])
assert index.read_bytes()==before and lib.read_bytes()==before_lib
