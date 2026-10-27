from pathlib import Path
import os,json,platform,subprocess
ROLES=['original','control','disabled','traced']
def env_for(role,trace):
 assert role in ROLES
 env=dict(os.environ,LIN_BENCH_SKIP_MYSQL='1',LIN_BENCH_SKIP_PG='1',LC_ALL='C',LIN_MUTATION_TRACE='1'if role=='traced'else '0',LIN_MUTATION_TRACE_PATH=str(trace));env.pop('LIN_BENCH_MIXED_CASE',None);return env
def capture(p):
 assert platform.system()=='Linux'and platform.machine()=='x86_64'
 assert not os.environ.get('LD_PRELOAD')and not os.environ.get('LD_AUDIT')and Path('/usr/bin/time').is_file()
 (p/'environment.json').write_text(json.dumps(dict(platform=platform.platform(),libc=platform.libc_ver(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),cpu=subprocess.check_output(['lscpu','-J'],text=True),allocator_environment={k:os.environ.get(k)for k in ['GLIBC_TUNABLES','MALLOC_ARENA_MAX','MALLOC_PERTURB_','LD_PRELOAD','LD_AUDIT']},allocator_unchanged_all_roles=True,trace_enabled_roles=['traced'],all_roles=ROLES),indent=2)+'\n')
