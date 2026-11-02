from pathlib import Path
import hashlib,json,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sys.path.insert(0,str(root/'scripts'));from benchmark_source_integrity import snapshot
sha=lambda b:hashlib.sha256(b).hexdigest();ref='60e64559ec0e5a21efd490d6da5a744e3ac8d0e2';assert not(p/'input.json').exists()
names=subprocess.check_output(['git','ls-tree','-r','--name-only',ref,'--','src','tests','lin-derive/src','tools/peer-worker/src'],cwd=root,text=True).splitlines();runtime={n:sha(subprocess.check_output(['git','show',ref+':'+n],cwd=root))for n in names}
for n,h in runtime.items():assert sha((root/n).read_bytes())==h,n
files={**runtime,**snapshot(root,None)['files_sha256']}
extra=[*p.glob('*.py'),p/'protocol.md',p/'runtime-inputs.json',root/'.github/workflows/cow-extent-mssql-private.yml',root/'.github/benchmark-recipients/mssql.age',root/'scripts/bench-peers-requirements.lock']
files.update({str(f.relative_to(root)):sha(f.read_bytes())for f in extra})
(p/'input.json').write_text(json.dumps(dict(runtime_reference=ref,runtime_source_sha256=runtime,files_sha256=files,features=['default','experimental-extent-wal'],entrypoint='root peer_bench example',upstream_correctness_run='41401044023',upstream_correctness_head='451fd7a25a6dc1f0f712eaef841aa8ee4bdd561d',image='mcr.microsoft.com/mssql/server@sha256:cf8f0ab2ebe61feec2eed2629061a07d247f791c2a8ddda74a4ac9f06a291718',numerical_outcomes_public=False,adopted=False),indent=2)+'\n')
print('Private current-source qualification inputs frozen')
