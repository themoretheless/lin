from pathlib import Path
import subprocess,hashlib,json,tempfile,tarfile
p=Path(__file__).resolve().parent;inputs=p/'container-input';results=p/'container-results';results.mkdir(exist_ok=True);h=lambda f:hashlib.sha256(Path(f).read_bytes()).hexdigest()
archive=Path('/Users/themoretheless/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/libc-0.2.189.crate');expected='3eaf3ede3fee6db1a4c2ee091bf8a8b4dccdc6d17f656fb07896ee72867612f2';assert h(archive)==expected
candidate=(p/'candidate-persist.rs').read_text();helper=(p/'helper.rs').read_text();assert helper in candidate
start=candidate.index('// Best-effort physical allocation only; logical WAL bytes/length and sync stay unchanged.');end=candidate.index('#[cfg(any(target_os = "linux", test))]',start)
(inputs/'src'/'reserve.rs').write_text(candidate[start:end]);(inputs/'src'/'helper.rs').write_text(helper);(inputs/'src'/'model.rs').write_text((p/'oracle.rs').read_text().split('#[cfg(all(test, target_os = "linux"))]')[0])
image='sha256:93ce27a88655056a51dbdd8f5f2d7ddc071c7b0070fb288a37b5a285fc83971e';meta=dict(image=image,candidate_sha256=h(p/'candidate-persist.rs'),helper_sha256=h(p/'helper.rs'),libc_package_checksum=expected,inputs_sha256={str(f.relative_to(inputs)):h(f)for f in inputs.rglob('*')if f.is_file()},standalone_backend_only=True,performance_qualified=False)
with tempfile.TemporaryDirectory(prefix='native-libc-',dir=p)as tmp:
 with tarfile.open(archive)as tar:
  for member in tar.getmembers():
   path=Path(member.name);assert not path.is_absolute()and '..'not in path.parts and path.parts[0]=='libc-0.2.189'and(member.isfile()or member.isdir()),member.name
  tar.extractall(tmp)
 libc=Path(tmp)/'libc-0.2.189'
 cmd=['docker','run','--rm','--network','none','-e','CARGO_HOME=/cargo','--mount',f'type=bind,source={inputs},target=/input,readonly','--mount',f'type=bind,source={results},target=/results','--mount',f'type=bind,source={libc},target=/libc,readonly','--mount','type=bind,source=/Users/themoretheless/.cargo/registry/index/index.crates.io-1949cf8c6b5b557f,target=/cargo/registry/index/index.crates.io-1949cf8c6b5b557f,readonly','--mount','type=bind,source=/Users/themoretheless/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f,target=/cargo/registry/cache/index.crates.io-1949cf8c6b5b557f,readonly',image,'sh','/input/runner.sh']
 with(p/'container.log').open('w')as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
meta['exit_code']=r.returncode;(p/'container-status.json').write_text(json.dumps(meta,indent=2)+'\n');assert r.returncode==0,'inspect container logs'
assert '6 passed; 0 failed;'in(results/'tests.log').read_text();assert int((results/'mutant-exit-code.txt').read_text())!=0 and 'test result: FAILED'in(results/'mutant.log').read_text()and 'error[E'not in(results/'mutant.log').read_text()
print('Linux ARM64 standalone backend:6 tests passed; KEEP_SIZE omission caught at runtime; no performance qualification')
