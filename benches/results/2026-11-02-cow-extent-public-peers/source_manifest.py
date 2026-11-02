from pathlib import Path
import sys
root=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(root/'scripts'))
from benchmark_source_integrity import snapshot
def manifest():
 return snapshot(root,None)['files_sha256']
