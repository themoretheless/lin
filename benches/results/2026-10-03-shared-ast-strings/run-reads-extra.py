import os,subprocess,pathlib
root=pathlib.Path(__file__).resolve().parent
for pair in range(4,7):
 for variant in (['baseline','candidate'] if pair%2 else ['candidate','baseline']):
  for case in ['fts_lex_common']:
   name=f'read-{case}-{variant}-{pair}'
   cmd=['/tmp/lin-shared-ast-'+variant,'--filter','compare/'+case+'/lin','--exact','--samples','32','--max-iterations','100','--warmup-ms','50','--sample-ms','5','--output',str(root/name)]
   with (root/(name+'.log')).open('w') as log:subprocess.run(cmd,env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1'),stdout=log,stderr=subprocess.STDOUT,check=True)
  print(pair,variant,flush=True)
