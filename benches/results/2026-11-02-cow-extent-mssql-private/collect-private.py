"""Collect only encrypted MSSQL evidence; never emit comparative outcomes."""
from pathlib import Path
import hashlib,json,os,shutil,stat,subprocess,sys,tarfile
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();run='41423609660';head='c56375ce27871332f10e675e10c49ac9184a05a4'
meta=json.loads(subprocess.check_output(['gh','run','view',run,'--json','databaseId,status,conclusion,headSha,url,jobs'],cwd=root,text=True))
assert meta['status']=='completed'and meta['conclusion']=='success'and meta['headSha']==head,'Functional job must finish; numerical outcomes are not inferred from status'
steps={s['name']:s['conclusion']for j in meta['jobs']for s in j['steps']}
for name in ['Verify frozen runtime and all workflow inputs','Audit exact upstream current-source Linux correctness','Build exact current root example with extent feature','Validate real MSSQL privately without exposing numerical outcomes','Encrypt every private artifact']:assert steps[name]=='success',name
vault=Path('/Users/themoretheless/Documents/Sources/lin/.airbug-bench/private-mssql');identity=vault/'identity.txt';assert vault.is_dir()and stat.S_IMODE(vault.stat().st_mode)==0o700 and stat.S_IMODE(identity.stat().st_mode)==0o600
private=vault/('cow-extent-'+run);out=root/'benches/results/2026-11-02-cow-extent-mssql-private-linux';assert not private.exists()and not out.exists(),'Preserve any previous collection attempt'
private.mkdir(mode=0o700);(private/'github-run.json').write_text(json.dumps(meta,indent=2)+'\n');(private/'run-metadata.json').write_text(json.dumps(dict(status=meta['status'],head_sha=head,id=int(run)),indent=2)+'\n')
download=private/'encrypted-download';diagnostics=private/'build-diagnostics';executables=private/'executable'
for name,destination in [('cow-extent-mssql-private-'+run,download),('cow-extent-mssql-build-diagnostics',diagnostics),('cow-extent-mssql-exact-linux-worker',executables)]:
 destination.mkdir(mode=0o700);subprocess.run(['gh','run','download',run,'--name',name,'--dir',str(destination)],cwd=root,check=True)
files=[f for f in download.rglob('*')if f.is_file()];assert len(files)==1 and files[0].name.endswith('.age');encrypted=files[0]
plaintext=private/'qualification.tar.gz';descriptor=os.open(plaintext,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
with os.fdopen(descriptor,'wb')as f:
 r=subprocess.run(['age','--decrypt','--identity',str(identity),str(encrypted)],stdout=f,stderr=subprocess.PIPE)
assert r.returncode==0,'Private decryption failed; no plaintext printed'
with tarfile.open(plaintext)as tar:
 members=tar.getmembers();assert members and all(not Path(m.name).is_absolute()and'..'not in Path(m.name).parts and(m.isdir()or m.isfile())and Path(m.name).parts[0]=='lin-cow-extent-mssql-private'for m in members)
 for member in members:
  destination=private/member.name
  if member.isdir():destination.mkdir(mode=0o700,parents=True,exist_ok=True)
  else:
   destination.parent.mkdir(mode=0o700,parents=True,exist_ok=True);descriptor=os.open(destination,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
   with os.fdopen(descriptor,'wb')as f:f.write(tar.extractfile(member).read())
payload=private/'lin-cow-extent-mssql-private';input_bytes=(payload/'frozen-input.json').read_bytes();assert input_bytes==subprocess.check_output(['git','show',head+':'+str(p.relative_to(root))+'/input.json'],cwd=root);inputs=json.loads(input_bytes)
for name,h in inputs['files_sha256'].items():assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
for name,h in inputs['runtime_source_sha256'].items():assert sha(subprocess.check_output(['git','show',inputs['runtime_reference']+':'+name],cwd=root))==h,name
build_bytes=(payload/'build-provenance.json').read_bytes();assert build_bytes==(diagnostics/'build-provenance.json').read_bytes();build=json.loads(build_bytes)
assert build['source_commit']==head and build['runtime_reference']==inputs['runtime_reference']=='60e64559ec0e5a21efd490d6da5a744e3ac8d0e2'and build['source_unchanged']
assert build['features']==inputs['features']==['default','experimental-extent-wal']and build['command']==['cargo','build','--locked','--release','--example','peer_bench','--features','experimental-extent-wal','--message-format=json-render-diagnostics']
assert 'release: 1.96.0'in build['compiler']and'host: x86_64-unknown-linux-gnu'in build['compiler']
for name,h in build['source_sha256'].items():assert sha(subprocess.check_output(['git','show',head+':'+name],cwd=root))==h,name
binaries=[f for f in executables.rglob('*')if f.is_file()];assert len(binaries)==1;b=binaries[0].read_bytes();assert sha(b)==build['binary']['sha256']and b[:6]==b'\x7fELF\x02\x01'and int.from_bytes(b[18:20],'little')==62
qa=json.loads((payload/'correctness-prerequisite.json').read_text());assert qa['exact_linux_correctness_verified']and qa['default_passed']==463 and qa['feature_passed']==483 and qa['compiled_negative_checks']==5 and qa['upstream']['databaseId']==41401044023 and qa['upstream']['headSha']=='451fd7a25a6dc1f0f712eaef841aa8ee4bdd561d'
env=json.loads((payload/'measurements/environment.json').read_text());assert inputs['image']in env['image']['RepoDigests']and env['binary_sha256']==build['binary']['sha256']
# This independently recomputes numerical data into the ignored private vault.
# Its stdout is intentionally generic; no win/loss result leaves the vault.
subprocess.run([sys.executable,str(p/'audit-private.py'),str(private)],cwd=root,check=True)
audit=json.loads((private/'private-audit.json').read_text());assert audit['functional_raw_audit_complete']and audit['samples']==1512 and len(audit['comparisons'])==21
out.mkdir();shutil.copy2(encrypted,out/'encrypted-results.tar.gz.age');subprocess.run(['age','-R',str(root/'.github/benchmark-recipients/mssql.age'),'-o',str(out/'independent-private-audit.json.age'),str(private/'private-audit.json')],check=True)
assert {str(f.relative_to(diagnostics))for f in diagnostics.rglob('*')if f.is_file()}=={'build.log','build-provenance.json'}
with tarfile.open(out/'public-build-diagnostics.tar.gz','w:gz')as tar:
 for f in sorted(diagnostics.iterdir()):tar.add(f,arcname=f.name,recursive=False)
status=dict(functional_raw_audit_complete=True,workflow_run=int(run),source_commit=head,runtime_reference=inputs['runtime_reference'],reports=6,original_processes=18,validated_samples=1512,scope_comparisons=21,exact_linux_elf_verified=True,source_and_build_git_blobs_verified=True,numerical_outcomes_public=False,production_adopted=False,full_objective_achieved=False)
(out/'functional-status.json').write_text(json.dumps(status,indent=2)+'\n');(out/'encrypted-artifact-sha256.json').write_text(json.dumps({f.name:sha(f.read_bytes())for f in sorted(out.iterdir())if f.is_file()},indent=2)+'\n')
print('Current-source MSSQL functional/raw audit and encrypted evidence preserved; numerical outcomes remain private')
