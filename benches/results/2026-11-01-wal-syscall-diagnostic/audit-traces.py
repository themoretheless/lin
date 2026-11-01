"""Reconstruct mutation-only syscall counts from raw traces; no latency claims."""
from pathlib import Path
import re,json,hashlib,collections,sys

def scope(text):
 lines=text.splitlines()
 start=[i for i,l in enumerate(lines)if re.search(r'\bwrite\(2<[^>]*>, "LIN_IO_BEGIN\\n", 13\)\s+= 13$',l)]
 end=[i for i,l in enumerate(lines)if re.search(r'\bwrite\(2<[^>]*>, "LIN_IO_END\\n", 11\)\s+= 11$',l)]
 assert len(start)==len(end)==1 and start[0]<end[0], 'missing/duplicate/invalid scope markers'
 calls=[]
 for line in lines[start[0]+1:end[0]]:
  assert '<unfinished ...>'not in line and ' resumed>'not in line,'split syscall needs explicit reconstruction'
  m=re.search(r'\b([a-z][a-z0-9_]*)\((.*)\)\s+= (-?\d+)(?:\s|$)',line)
  assert m,('unrecognized scoped trace',line)
  call,args,result=m.groups();result=int(result);assert result>=0,('failed scoped syscall',line)
  fd=re.match(r'(\d+)<([^>]+)>',args)
  calls.append(dict(call=call,result=result,fd=int(fd[1])if fd else None,path=fd[2]if fd else None,raw=line))
 return calls

def audit(p):
 provenance=json.loads((p/'provenance.json').read_text());assert provenance['source_restored']
 frozen=json.loads((p/'frozen.json').read_text())
 assert provenance['harness_sha256']==frozen['benches/results/2026-11-01-wal-syscall-diagnostic/diagnostic-compare.rs']
 assert all(frozen[k]==v for k,v in provenance['source_sha256'].items())
 ordinary=json.loads((p/'functional/status.json').read_text());traced=json.loads((p/'traces/status.json').read_text())
 expected=[(n,op,e)for n in [1000,10000]for op in ['update','delete','commit','rollback']for e in ['lin','sqlite_wal']]
 for status in [ordinary,traced]:
  assert status['complete']and not status['performance_qualification']
  assert [(r['rows'],r['operation'],r['engine'])for r in status['cases']]==expected
  assert all(r['exit_code']==0 and r['full_values_reopen']for r in status['cases'])
 output=[]
 for r in traced['cases']:
  name=f"{r['rows']}-{r['operation']}-{r['engine']}";trace=p/'traces'/(name+'.strace');data=trace.read_bytes();assert hashlib.sha256(data).hexdigest()==r['trace_sha256']
  for folder in ['traces','functional']:
   assert 'LIN_IO_FULL_VALUES_REOPEN_OK'in(p/folder/(name+'.stdout')).read_text()
  calls=scope(data.decode());counts=dict(collections.Counter(x['call']for x in calls));assert counts==r['scope_syscalls']
  writes=[x for x in calls if x['call']in ['write','writev','pwrite64','pwritev']]
  syncs=[x for x in calls if x['call']in ['fsync','fdatasync']]
  output.append(dict(case=name,counts=counts,successful_write_bytes=sum(x['result']for x in writes),writes=writes,syncs=syncs))
 result=dict(complete=True,cases=output,performance_qualification=False,full_objective_achieved=False,binary_payload_rehashed=False)
 (p/'independent-trace-audit.json').write_text(json.dumps(result,indent=2)+'\n')
 lines=['# Lin / SQLite WAL FULL syscall observations','','Single fresh invocation per case. Setup and close/reopen are outside markers. Traced execution is not a performance comparison. Counts and bytes do not establish crash safety or explain latency by themselves.','','|Case|Write calls|Successful write bytes|fdatasync|fsync|','|---|---:|---:|---:|---:|']
 for r in output:lines.append(f"|{r['case']}|{len(r['writes'])}|{r['successful_write_bytes']}|{r['counts'].get('fdatasync',0)}|{r['counts'].get('fsync',0)}|")
 (p/'syscall-report.md').write_text('\n'.join(lines)+'\n')
 print(json.dumps({'cases':len(output),'raw_trace_reconstruction':True}))
if __name__=='__main__':audit(Path(sys.argv[1]).resolve()if len(sys.argv)>1 else Path(__file__).resolve().parent)
