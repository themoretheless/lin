from pathlib import Path
import json,statistics,sys
p=Path(sys.argv[1]).resolve()if len(sys.argv)>1 else Path(__file__).resolve().parent;root=p/'benchmark-measurement';summary=json.loads((root/'summary.json').read_text());q=json.loads((root/'qualification.json').read_text());audit=json.loads((p/'benchmark-independent-audit.json').read_text());assert q['measurement_complete']and q['processes']==864 and q['observations']==6912 and audit['complete']
workloads=[(op,size,f'compare/durable_{op}_1row_{size}/')for size in ['1k','10k']for op in ['update','delete','commit']]+[('column_batch_full_insert',size,f'compare/column_batch_full_insert_{size}/')for size in ['1k','2k']];rows=[]
for op,size,prefix in workloads:
 pairs=[]
 for series in ['series1','series2']:
  left=summary[prefix+'lin'][series]['pairs'];right=summary[prefix+'sqlite_wal'][series]['pairs'];assert len(left)==len(right)==6
  for a,b in zip(left,right):
   assert a['round']==b['round'];ratio=a['candidate_ns']/b['candidate_ns'];pairs.append(dict(series=series,round=a['round'],lin_ns=a['candidate_ns'],sqlite_ns=b['candidate_ns'],lin_over_sqlite=ratio,lin_numeric_win=ratio<1))
 rows.append(dict(operation=op,size=size,pairs=pairs,numeric_wins=sum(r['lin_numeric_win']for r in pairs),total_pairs=12,median_ratio=statistics.median(r['lin_over_sqlite']for r in pairs)))
result=dict(comparisons=rows,numeric_wins=sum(r['numeric_wins']for r in rows),total_pairs=96,component_gate_unchanged=q['component_gate_passes'],full_objective_achieved=False,qualification='Numeric cross-case ratios from separate fresh processes; no acceptance waiver or additional confidence interval');(root/'sqlite-peer-ratios.json').write_text(json.dumps(result,indent=2)+'\n')
lines=['# Columnar Batch candidate / SQLite WAL FULL','','Original complete matrix, separate fresh processes. Ratio < 1 is a numeric Lin win. New grouped inserts and existing single-row writes retain their own declared workloads. No whole-engine or eight-peer qualification.','','|Operation|Rows|Lin/SQLite median paired ratio|Numeric wins|','|---|---|---:|---:|']
for r in rows:lines.append(f"|{r['operation']}|{r['size']}|{r['median_ratio']:.6f}|{r['numeric_wins']}/12|")
(root/'sqlite-peer-ratios.md').write_text('\n'.join(lines)+'\n');print(json.dumps(dict(numeric_wins=result['numeric_wins'],total_pairs=96,full_objective_achieved=False)))
