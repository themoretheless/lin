"""Report external numeric ratios from the completed matrix without changing its gate."""
from pathlib import Path
import json,statistics,sys
p=Path(sys.argv[1]).resolve()if len(sys.argv)>1 else Path(__file__).resolve().parent
summary=json.loads((p/'measurement/summary.json').read_text());q=json.loads((p/'measurement/qualification.json').read_text());audit=json.loads((p/'independent-audit.json').read_text())
assert q['measurement_complete'] and q['processes']==720 and q['observations']==5760 and audit['complete']
comparisons=[]
for size in ['1k','10k']:
 for operation in ['update','delete','commit']:
  prefix=f'compare/durable_{operation}_1row_{size}/';left=summary[prefix+'lin'];right=summary[prefix+'sqlite_wal'];pairs=[]
  for series in ['series1','series2']:
   a=left[series]['pairs'];b=right[series]['pairs'];assert len(a)==len(b)==6
   for lin,sqlite in zip(a,b):
    assert lin['round']==sqlite['round'];ratio=lin['candidate_ns']/sqlite['candidate_ns'];pairs.append(dict(series=series,round=lin['round'],lin_ns=lin['candidate_ns'],sqlite_ns=sqlite['candidate_ns'],lin_over_sqlite=ratio,lin_numeric_win=ratio<1))
  comparisons.append(dict(size=size,operation=operation,pairs=pairs,numeric_wins=sum(r['lin_numeric_win']for r in pairs),total_pairs=len(pairs),median_ratio=statistics.median(r['lin_over_sqlite']for r in pairs)))
result=dict(comparisons=comparisons,numeric_wins=sum(r['numeric_wins']for r in comparisons),total_pairs=72,full_objective_achieved=False,component_gate_unchanged=q['component_gate_passes'],qualification='Numeric cross-case paired-process ratios only; no additional confidence interval or acceptance waiver')
(p/'measurement/sqlite-peer-ratios.json').write_text(json.dumps(result,indent=2)+'\n')
lines=['# Linux fsync candidate / SQLite WAL FULL','','Numeric ratios from the original complete matrix. Lin and SQLite use separate fresh processes. This report does not override the frozen component gate or qualify all peers. Ratio below 1 is a numeric Lin win.','','|Operation|Rows|Lin/SQLite median paired ratio|Numeric wins|','|---|---|---:|---:|']
for r in comparisons:lines.append(f"|{r['operation']}|{r['size']}|{r['median_ratio']:.6f}|{r['numeric_wins']}/12|")
(p/'measurement/sqlite-peer-ratios.md').write_text('\n'.join(lines)+'\n')
print(json.dumps({'numeric_wins':result['numeric_wins'],'total_pairs':72,'full_objective_achieved':False}))
