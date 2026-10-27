from pathlib import Path
import os,json,subprocess,platform,re
KEY='glibc.malloc.tcache_unsorted_limit'
def env_for(role):
 assert role in ['baseline','control','candidate'];env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None)
 if role=='candidate':env['GLIBC_TUNABLES']=':'.join([s for s in(env.get('GLIBC_TUNABLES')or'').split(':')if s and s.split('=',1)[0]!=KEY]+[KEY+'=16'])
 return env
def capture(p):
 assert platform.system()=='Linux'and platform.machine()=='x86_64'and platform.libc_ver()==('glibc','2.39')
 assert not os.environ.get('LD_PRELOAD')and not os.environ.get('LD_AUDIT');loader=Path('/lib64/ld-linux-x86-64.so.2');assert loader.is_file()and Path('/usr/bin/time').is_file();out={}
 for role in ['baseline','control','candidate']:
  env=env_for(role);text=subprocess.check_output([str(loader),'--list-tunables'],env=env,text=True);(p/f'loader-{role}.txt').write_text(text);values={}
  for line in text.splitlines():
   match=re.match(r'^(glibc\.malloc\.[a-z_]+):\s*(0x[0-9a-fA-F]+|[0-9]+)\b',line)
   if match:values[match[1]]=int(match[2],0)
  assert KEY in values;out[role]=dict(glibc_tunables=env.get('GLIBC_TUNABLES'),loader_reported_malloc=values)
 assert out['baseline']==out['control']and out['baseline']['loader_reported_malloc'][KEY]==0 and out['candidate']['loader_reported_malloc'][KEY]==16
 assert {k:v for k,v in out['baseline']['loader_reported_malloc'].items()if k!=KEY}=={k:v for k,v in out['candidate']['loader_reported_malloc'].items()if k!=KEY}
 (p/'effective-tunables.json').write_text(json.dumps(out,indent=2)+'\n');(p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),libc=platform.libc_ver(),cpu_count=os.cpu_count(),cpu_info=Path('/proc/cpuinfo').read_text().split('\n\n')[0],rustc=subprocess.check_output(['rustc','-Vv'],text=True),gnu_time=subprocess.check_output(['/usr/bin/time','--version'],text=True),allocator_environment={k:os.environ.get(k)for k in ['GLIBC_TUNABLES','MALLOC_ARENA_MAX','MALLOC_PERTURB_','MALLOC_MMAP_THRESHOLD_','MALLOC_TRIM_THRESHOLD_','LD_PRELOAD','LD_AUDIT']}),indent=2)+'\n')
 return out
