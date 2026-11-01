from pathlib import Path
import os,json,platform,subprocess
def env_for(role):
 assert role in ['baseline','control','candidate']
 env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C');env.pop('LIN_BENCH_MIXED_CASE',None);return env
def capture(p):
 assert platform.system()=='Linux'and platform.machine()=='x86_64'
 assert not os.environ.get('LD_PRELOAD')and not os.environ.get('LD_AUDIT')
 assert Path('/usr/bin/time').is_file()
 (p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),libc=platform.libc_ver(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),allocator_environment={k:os.environ.get(k)for k in ['GLIBC_TUNABLES','MALLOC_ARENA_MAX','MALLOC_PERTURB_','LD_PRELOAD','LD_AUDIT']},runtime_unchanged_all_roles=True),indent=2)+'\n')
