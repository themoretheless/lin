from pathlib import Path
import json,shutil
r=Path(__file__).resolve().parent
for mode in ['aa','ab']:shutil.copytree(r/f'interleaved-{mode}',r/f'interleaved-{mode}-s2')
for mode in ['aa','ab','aa-s2','ab-s2']:
 base=mode.split('-')[0];p=r/f'boundary-{mode}';p.mkdir();source=(r/f'interleaved-{base}/run.py').read_text();assert 'for n in [1000,10000]:' in source;source=source.replace('for n in [1000,10000]:','for n in [682,683]:');(p/'run.py').write_text(source);shutil.copy2(r/f'interleaved-{base}/provenance.json',p/'provenance.json')
