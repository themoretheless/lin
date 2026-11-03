from pathlib import Path
import hashlib,json
assets=Path(__file__).resolve().parent;root=assets.parents[2]
def manifest():
 inputs=json.loads((assets/'input.json').read_text());result={}
 for name,digest in inputs['files_sha256'].items():
  value=hashlib.sha256((root/name).read_bytes()).hexdigest();assert value==digest,name;result[name]=value
 assert {str(p.relative_to(root)) for base in ['src','lin-derive/src','tools/peer-worker/src'] for p in (root/base).rglob('*.rs')}==set(inputs['rust_source_files'])
 return result
