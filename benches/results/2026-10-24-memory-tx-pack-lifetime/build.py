from pathlib import Path
import subprocess,json,hashlib,shutil
p=Path(__file__).resolve().parent
status={}
for key,cmd in [("transaction-tests",["cargo","test","--test","transaction_tests"]),("build",["cargo","build","--release","--example","peer_bench"])]:
 with (p/(key+".log")).open("w") as log:r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT)
 status[key]=r.returncode;(p/"checks.json").write_text(json.dumps(status));print(key,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
shutil.copy2("target/release/examples/peer_bench","/tmp/lin-memory-tx-pack-candidate")
(p/"binary-sha256.json").write_text(json.dumps({k:hashlib.sha256(Path(v).read_bytes()).hexdigest() for k,v in {"baseline":"/tmp/lin-bounded-native-insert","candidate":"/tmp/lin-memory-tx-pack-candidate"}.items()},indent=2))
