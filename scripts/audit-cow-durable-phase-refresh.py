#!/usr/bin/env python3
"""Audit the frozen completed COW phase diagnostic without resetting current source."""
from pathlib import Path
import argparse,subprocess,json,hashlib,tempfile
ROOT=Path(__file__).resolve().parents[1]
HEAD='75f8f6b7f7f03dc22a64cb963264f33e69f7da9d'
INPUT='benches/results/2026-10-28-cow-durable-phase-refresh'
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('result_directory',type=Path);args=parser.parse_args();parent=args.result_directory.resolve();artifact=parent/'cow-durable-phase-refresh'
 metadata=json.loads((parent/'run-metadata.json').read_text());assert metadata['status']=='completed'and metadata['headSha']==HEAD and metadata['conclusion']in ['success','failure'],'Only the completed frozen run can be audited'
 def git(name):
  path=Path(name);assert not path.is_absolute()and'..'not in path.parts
  return subprocess.check_output(['git','show',HEAD+':'+name],cwd=ROOT)
 manifest=(artifact/'input-provenance.json').read_bytes();assert manifest==git(INPUT+'/input-provenance.json'),'Input manifest drift';m=json.loads(manifest)
 for name,digest in m['files_sha256'].items():assert hashlib.sha256((artifact/name).read_bytes()).hexdigest()==digest,name
 records=json.loads((artifact/'process-status.json').read_text());assert len(records)==240,'Incomplete diagnostic matrix must remain unqualified'
 with tempfile.TemporaryDirectory(prefix='lin-cow-phase-frozen-')as td:
  snapshot=Path(td)
  for section in ['source_sha256','build_inputs_sha256']:
   for name,digest in m[section].items():
    data=git(name);assert hashlib.sha256(data).hexdigest()==digest,name;target=snapshot/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
  result=subprocess.run(['python3',str(artifact/'verify.py')],cwd=snapshot,capture_output=True,text=True)
  (parent/'frozen-audit.log').write_text(result.stdout+result.stderr);assert result.returncode==0,result.stderr
 audit=json.loads((artifact/'raw-source-audit.json').read_text());assert audit['audit_complete']and not audit['source_candidate_qualified']and not audit['full_objective_achieved']
 summary=json.loads((artifact/'summary.json').read_text());assert len(summary)==10
 result=dict(audit_complete=True,run_head=HEAD,processes=240,observations=1920,source_candidate_qualified=False,full_objective_achieved=False,scope='Frozen current-COW caller-thread phase diagnostic; observer overhead retained; no production optimization qualification',phase_summary={case:dict(aa_absolute_percent=r['aa_absolute_percent'],disabled_over_original_ratio=r['disabled_over_original_ratio'],traced_over_disabled_ratio=r['traced_over_disabled_ratio'],trace_records=r['trace_records'],payload_bytes=r['payload_bytes'],same_invocation_phase_percent=r['same_invocation_phase_percent'])for case,r in summary.items()})
 (parent/'frozen-audit.json').write_text(json.dumps(result,indent=2)+'\n');(parent/'audited-report.md').write_bytes((artifact/'report.md').read_bytes());print(json.dumps(result,indent=2))
if __name__=='__main__':main()
