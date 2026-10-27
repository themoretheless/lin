from pathlib import Path
from collections import defaultdict
import json,statistics,math,hashlib
p=Path(__file__).resolve().parent;out={}
for mode,ncases in [('ci',71),('ci-mutations',20)]:
 d=json.loads((p/mode/'run.json').read_text());assert d['status']=='complete' and len(d['cases'])==ncases and len(d['observations'])==ncases*8
 values=defaultdict(list)
 for o in d['observations']:
  assert o['metric']=='wall' and o['operations']>0
  v=float(o['value'])/o['operations'];assert math.isfinite(v)and v>0;values[o['case']].append(v)
 assert all(len(v)==8 for v in values.values());med={c:statistics.median(v)for c,v in values.items()};comparisons=[]
 for c,t in med.items():
  if not c.endswith('/lin'):continue
  for peer in ['sqlite','duckdb']:
   other=c[:-3]+peer
   if other in med:comparisons.append({'case':c,'peer':peer,'lin_ns':t,'peer_ns':med[other],'lin_over_peer':t/med[other],'lin_wins':t<med[other]})
 out[mode]={'observations':len(d['observations']),'comparisons':comparisons,'wins':sum(r['lin_wins']for r in comparisons),'losses':sum(not r['lin_wins']for r in comparisons)}
(p/'verified-summary.json').write_text(json.dumps(out,indent=2)+'\n');(p/'sha256.json').write_text(json.dumps({str(f.relative_to(p)):hashlib.sha256(f.read_bytes()).hexdigest()for f in p.rglob('*')if f.is_file()and f.name!='sha256.json'},indent=2)+'\n')
for mode,d in out.items():
 print(mode,d['wins'],d['losses'])
 for r in d['comparisons']:
  if not r['lin_wins']:print(r['case'],r['peer'],round(r['lin_over_peer'],3))
