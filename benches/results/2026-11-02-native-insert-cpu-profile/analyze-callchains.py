from pathlib import Path
from collections import Counter
import hashlib,json,re
p=Path(__file__).resolve().parent;root=p.parents[2];raw=Path('/tmp/lin-native-insert-profile-41396688130');out=root/'benches/results/2026-11-02-native-insert-cpu-profile-linux'
text=(raw/'perf-script.txt').read_text();samples=[]
for block in text.strip().split('\n\n'):
 lines=block.splitlines();m=re.search(r'\s(\d+) cpu-clock:u:',lines[0])
 if not m:continue
 frames=[]
 for line in lines[1:]:
  mframe=re.match(r'\s+[0-9a-f]+ (.*) \(.*\)$',line)
  if mframe:frames.append(re.sub(r'\+0x[0-9a-f]+$','',mframe[1]))
 if frames:samples.append((int(m[1]),frames))
selected=[s for s in samples if 'lin::exec::Transaction::run_prepared' in s[1]]
assert len(selected)>500
inclusive=Counter();leaf=Counter();total=sum(w for w,_ in selected)
for w,frames in selected:
 leaf[frames[0]]+=w
 for symbol in set(frames):inclusive[symbol]+=w
result=dict(scope='Only callchains containing Transaction::run_prepared; excludes preparation, validation, output, destruction and transaction begin/commit without this ancestor, including spawned-thread stacks without the parent frame. Inclusive categories overlap. Not a complete timed-region profile or peer comparison.',raw_script_sha256=hashlib.sha256((raw/'perf-script.txt').read_bytes()).hexdigest(),whole_worker_samples=len(samples),selected_samples=len(selected),selected_period=total,inclusive=[dict(symbol=s,period=w,selected_share_percent=w/total*100)for s,w in inclusive.most_common()],leaf=[dict(symbol=s,period=w,selected_share_percent=w/total*100)for s,w in leaf.most_common()])
(out/'insert-callchains.json').write_text(json.dumps(result,indent=2)+'\n')
for s in ['lin::exec::Db::maintain_insert_slab','lin::fts::FtsIndex::append_slab','lin::store::Store::index_insert_slab','lin::exec::Db::append_batch_shape_rows','lin::exec::Db::insert_bulk']:
 print(s,round(inclusive[s]/total*100,2))
print('samples',len(selected),'/',len(samples));print('\n'.join(f'{w/total*100:.2f}% {s}'for s,w in inclusive.most_common(25)))
