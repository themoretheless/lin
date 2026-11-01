from pathlib import Path
import json
p=Path(__file__).resolve().parent;a=json.loads((p/'audit.json').read_text())
lines=['# Corrected COW: available-peer API comparison','',f"Source `{a['source_commit']}`, runtime625bc7180. macOS ARM64 with native ARM server containers on Linux6.8; complete values/columns/multiplicity; three fresh processes at each of1k/10k/100k;12 calibrated samples per case/engine/process. No previous-host causal speedup inference.",'','Prepared Rust Lin versus Python APIs and server drivers/network is the declared contract. Server tmpfs/native insertion contracts differ from Full durable writes. Embeddings and FTS stay enabled. All losing comparisons remain. MSSQL private comparisons and real Kusto are outside this run; the full eight-peer objective remains open.','',', '.join(f'{k}: {v}'for k,v in a['versions'].items())+'.','']
for result in a['results']:
 lines += [f"## {result['mode']}",'',f"Aggregate numerical wins: {result['wins']}/{len(result['comparisons'])}; positive process comparisons: {result['positive_processes']}/{result['process_comparisons']}.",'','| Rows | Case | Peer | Lin ms | Peer ms | Peer/Lin | Positive processes |','|---:|---|---|---:|---:|---:|---:|']
 for r in result['comparisons']:lines.append(f"| {r['rows']} | {r['case']} | {r['peer']} | {r['lin_ns']/1e6:.6f} | {r['peer_ns']/1e6:.6f} | {r['speedup']:.3f} | {r['positive_processes']}/3 |")
 lines.append('')
(p/'report.md').write_text('\n'.join(lines)+'\n')
