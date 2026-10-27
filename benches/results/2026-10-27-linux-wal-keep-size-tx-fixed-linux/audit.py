from pathlib import Path
import hashlib,json,subprocess,tempfile,re
p=Path(__file__).resolve().parent;root=p.parents[2];artifact=p/'linux-wal-keep-size-transaction-fixed';run=json.loads((p/'run-metadata.json').read_text())
assert run['status']=='completed'and run['conclusion']=='success'and run['headSha']=='08efc6e7f526ccefd62050f4b9266351c95fbde5'
m=json.loads((artifact/'test-status.json').read_text());assert m['linux_backend_executed']and m['exit_code']==0 and m['source_restored']and not m['performance_qualified']
# Verify against immutable Git objects, without replacing current project sources.
with tempfile.TemporaryDirectory(prefix='lin-reserve-source-audit-')as tmp:
 t=Path(tmp)
 for name,digest in m['source_sha256'].items():
  data=subprocess.check_output(['git','show',run['headSha']+':'+name],cwd=root);assert hashlib.sha256(data).hexdigest()==digest,name
  dest=t/name;dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(data)
 subprocess.run(['python3',str(artifact/'verify.py')],cwd=t,check=True)
text=(artifact/'tests.log').read_text();counts=[tuple(map(int,r))for r in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)];assert sum(r[0]for r in counts)==421 and sum(r[1]for r in counts)==0 and sum(r[2]for r in counts)==2
probes=[json.JSONDecoder().raw_decode(part)[0]for part in text.split('LIN_KEEP_SIZE_PROBE ')[1:]];assert len(probes)==2
(p/'verified-probes.json').write_text(json.dumps(probes,indent=2)+'\n')
print('Independent audit PASS: exact historical core,421 Linux release tests /2 ignored,actual KEEP_SIZE and follower/replay/checkpoint oracles,runtime mutant; no latency claim')
