from pathlib import Path
import json,platform,re,os
# Preserve the repository's previously qualified Mongo image/kernel guard.
def kernel_affected(release):
 m=re.match(r'^(\d+)\.(\d+)\.(\d+)(?:[-+.]|$)',release)
 if not m or 'rc' in release:raise ValueError('Unrecognized or prerelease kernel')
 return (6,19,0)<=tuple(map(int,m.groups()))<(7,0,14)
if __name__=='__main__':
 assert platform.system()=='Linux' and platform.machine()=='x86_64'
 out=Path('/tmp/lin-current-candidate-public-peers-20261103');out.mkdir(exist_ok=True)
 affected=kernel_affected(platform.release());(out/'host-check.json').write_text(json.dumps({'platform':platform.platform(),'kernel':platform.release(),'kernel_guard_passed':not affected},indent=2)+'\n');assert not affected,'Known affected kernel: do not start MongoDB'
