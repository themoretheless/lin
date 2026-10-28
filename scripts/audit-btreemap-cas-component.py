#!/usr/bin/env python3
"""Audit frozen API-compatible BTreeMap CAS component data without changing a checkout."""
from pathlib import Path
import argparse,subprocess,json,hashlib,tempfile
ROOT=Path(__file__).resolve().parents[1]
HEAD='57dd672f1077ae8bb56c35076072ea5d1e025dc3'
INPUT='benches/results/2026-10-28-btreemap-single-stmt-cas'
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('result_directory',type=Path);args=parser.parse_args();parent=args.result_directory.resolve();raw=parent/'btreemap-cas-component'
 metadata=json.loads((parent/'run-metadata.json').read_text());assert metadata['status']=='completed'and metadata['headSha']==HEAD and metadata['conclusion']in ['success','failure']
 def git(name,ref=HEAD):
  f=Path(name);assert not f.is_absolute()and'..'not in f.parts
  return subprocess.check_output(['git','show',ref+':'+name],cwd=ROOT)
 manifest=(raw/'input-provenance.json').read_bytes();assert manifest==git(INPUT+'/input-provenance.json');m=json.loads(manifest)
 assert m['cow_included']is False and m['row_alias_preserved']and m['baseline_reference']=='6812cc5ca9fc35a4adc81e0948df45dfe3629742'
 for name,digest in m['files_sha256'].items():assert hashlib.sha256((raw/name).read_bytes()).hexdigest()==digest,name
 assert (raw/'baseline-exec.rs').read_bytes()==git('src/exec.rs',m['baseline_reference'])
 assert (raw/'candidate-exec.rs').read_bytes()==git('src/exec.rs')
 assert len(json.loads((raw/'process-status.json').read_text()))==144,'Incomplete raw matrix must not qualify'
 with tempfile.TemporaryDirectory(prefix='lin-btreemap-cas-frozen-')as td:
  snapshot=Path(td)
  for section in ['source_sha256','build_inputs_sha256']:
   for name,digest in m[section].items():
    data=git(name);assert hashlib.sha256(data).hexdigest()==digest,name;target=snapshot/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
  result=subprocess.run(['python3',str(raw/'audit.py')],cwd=snapshot,capture_output=True,text=True);(parent/'frozen-audit.log').write_text(result.stdout+result.stderr);assert result.returncode==0,result.stderr
  audit=json.loads(result.stdout)
 assert audit['audit_complete']and audit['processes']==144 and audit['observations']==1152
 q=json.loads((raw/'qualification.json').read_text());assert not q['full_engine_qualified']and not q['full_objective_achieved']
 audit.update(run_head=HEAD,row_alias_preserved=True,cow_included=False,full_engine_qualified=False,full_objective_achieved=False)
 (parent/'frozen-audit.json').write_text(json.dumps(audit,indent=2)+'\n');(parent/'audited-report.md').write_bytes((raw/'report.md').read_bytes());print(json.dumps(audit,indent=2))
if __name__=='__main__':main()
