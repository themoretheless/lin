"""Audit encrypted MSSQL evidence; only functional counts may reach stdout."""
from pathlib import Path
import argparse,subprocess,tarfile,json,os,hashlib,math,statistics as st
parser=argparse.ArgumentParser();parser.add_argument('--vault',type=Path,required=True);parser.add_argument('--identity',type=Path,required=True);args=parser.parse_args();vault=args.vault;enc=vault/'encrypted/lin-mssql-results.tar.gz.age';plain=vault/'results.tar.gz'
data=subprocess.check_output(['age','-d','-i',str(args.identity),str(enc)])
if plain.exists():assert plain.read_bytes()==data
else:plain.write_bytes(data)
os.chmod(plain,0o600)
with tarfile.open(plain,'r:gz') as tf:
 assert all((m.isfile() or m.isdir()) and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts and Path(m.name).parts[0]=='lin-mssql-private' for m in tf)
 tf.extractall(vault,filter='data')
for f in vault.rglob('*'):os.chmod(f,0o700 if f.is_dir() else 0o600)
p=vault/'lin-mssql-private';load=lambda f:json.loads(f.read_text());m=load(p/'environment.json');assert m['status']=='finished' and m['source_unchanged'] and m['source_commit']=='c739242ca5ba08fa719865d38b76177757dc0b1d';assert m['architecture']=='x86_64' and m['cleanup']=={'stop_exit':0,'remove_exit':0};assert len(m['runs'])==6
for name,digest in m['source_sha256'].items():assert hashlib.sha256(subprocess.check_output(['git','show',m['source_commit']+':'+name])).hexdigest()==digest
count=processes=comparisons=0;private_comparisons=[]
for mode in ['read','native']:
 cases=['point_get','filter_eq','text_substr','materialize','join_inner','join_filter'] if mode=='read' else ['insert_native']
 for n in [1000,10000,100000]:
  run=p/mode/f'rows-{n}';d=load(run/'run.json');assert d['status']=='complete' and not d['errors'] and not d['source_drift'];assert set(d['engines'])=={'lin','mssql'} and d['cases']==cases and d['repeats']==3 and d['samples']==12;assert d['lin_binary_info']['sha256']==m['binary_sha256'];assert d['git_commit']==m['source_commit']
  r=next(x for x in m['runs'] if x['mode']==mode and x['rows']==n);assert r['sentinel_unchanged'] and not r['fixture_objects_remaining'] and r['exit_code'] in [0,3]
  coords=set();med={}
  for o in d['observations']:
   c=(o['case'],o['repetition']);assert c not in coords;coords.add(c);vals=o['samples_ns'];assert len(vals)==12 and o['iterations']>0 and all(math.isfinite(v) and v>0 for v in vals);med[c]=st.median(vals);count+=12
  assert coords=={(c+'/'+e,i) for c in cases for e in ['lin','mssql'] for i in range(3)}
  for i in range(3):
   child=load(run/f'process-{i+1}'/'run.json');assert child['status']=='complete' and not child['errors'] and not child['source_drift'] and child['lin_binary_info']['sha256']==m['binary_sha256'];processes+=1
   for o in child['observations']:assert med[o['case'],i]==st.median(o['samples_ns'])
  for c in cases:
   vals={e:st.median(med[c+'/'+e,i] for i in range(3)) for e in ['lin','mssql']};assert all(vals[e]==d['medians_ns'][c+'/'+e] for e in vals);private_comparisons.append({'mode':mode,'rows':n,'case':c,'peer_over_lin':vals['mssql']/vals['lin'],'process_ratios':[med[c+'/mssql',i]/med[c+'/lin',i] for i in range(3)]});comparisons+=1
assert (count,processes,comparisons)==(1512,18,21)
private={'functional_audit_passed':True,'runtime':m['source_commit'],'processes':processes,'wall_samples':count,'comparisons':private_comparisons,'all_aggregate_wins':all(c['peer_over_lin']>1 for c in private_comparisons),'all_process_wins':all(all(r>1 for r in c['process_ratios']) for c in private_comparisons)}
private['raw_manifest']={str(f.relative_to(p)):{'bytes':f.stat().st_size,'sha256':hashlib.sha256(f.read_bytes()).hexdigest()} for f in p.rglob('*') if f.is_file()}
(vault/'independent-audit.json').write_text(json.dumps(private,indent=2));os.chmod(vault/'independent-audit.json',0o600)
subprocess.run(['age','-R','.github/benchmark-recipients/mssql.age','-o',str(vault/'independent-audit.json.age'),str(vault/'independent-audit.json')],check=True)
print('Candidate MSSQL functional/source/cleanup audit passed: 18 processes, 1512 observations; numerical results remain private')
