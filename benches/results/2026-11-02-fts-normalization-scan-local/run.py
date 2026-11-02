from pathlib import Path
import hashlib,json,platform,statistics,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2]
assert not(p/'raw.json').exists(),'preserve prior experiment'
old=subprocess.check_output(['git','show','4047596cd:src/fts.rs'],cwd=root,text=True);new=(root/'src/fts.rs').read_text()
def function(s,name):
 a=s.index("pub(crate) fn lowercased<'a>");b=s.index('\n}\n',a)+2
 return '#[inline(never)]\n'+s[a:b].replace('pub(crate) fn lowercased',f'fn {name}')
source='use std::{hint::black_box,time::Instant};\n'+function(old,'baseline')+'\n'+function(new,'candidate')+r'''
fn main(){let args:Vec<_>=std::env::args().collect();let f:for<'a>fn(&'a str,&'a mut String)->&'a str=if args[1]=="cand"{candidate}else{baseline};let text=match args[2].as_str(){"short_lower"=>"doc 99999 wal note".to_string(),"body_lower"=>"body 99999".to_string(),"long_lower"=>"lowercase words 123 ".repeat(64),"short_upper"=>"DOC 99999 WAL NOTE".to_string(),"late_upper"=>format!("{}Z","lower words ".repeat(64)),"unicode_lower"=>"ёж é 🦔 ".repeat(16),"unicode_upper"=>"ЁЖ İ ΣΟΣ ".repeat(16),_=>panic!()};let expected:String=text.chars().flat_map(char::to_lowercase).collect();let mut scratch=String::new();assert_eq!(f(&text,&mut scratch),expected);let start=Instant::now();for _ in 0..200000{black_box(f(black_box(&text),black_box(&mut scratch)));}let elapsed=start.elapsed().as_nanos();assert_eq!(f(&text,&mut scratch),expected);println!("{}",elapsed);}
'''
(p/'micro.rs').write_text(source);binary=p/'micro';command=['rustc','-O','--edition=2024',str(p/'micro.rs'),'-o',str(binary)];subprocess.run(command,cwd=root,check=True)
sys.path.insert(0,str(root/'scripts'));from benchmark_role_balance import role_order
cases=['short_lower','body_lower','long_lower','short_upper','late_upper','unicode_lower','unicode_upper'];raw=[]
for rnd in range(1,7):
 for ci,case in enumerate(cases):
  for role in role_order(rnd,ci,['base','cont','cand']):
   cmd=[str(binary),role,case];value=int(subprocess.check_output(cmd,text=True));assert value>0;raw.append(dict(round=rnd,case=case,role=role,ns=value,command=cmd))
(p/'raw.json').write_text(json.dumps(raw,indent=2)+'\n')
summary={}
for case in cases:
 gains=[];aa=[]
 for rnd in range(1,7):
  vals={r['role']:r['ns'] for r in raw if r['case']==case and r['round']==rnd};gains.append((1-vals['cand']/vals['base'])*100);aa.append(abs(vals['cont']/vals['base']-1)*100)
 summary[case]=dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(v>0 for v in gains),gains_percent=gains)
(p/'summary.json').write_text(json.dumps(dict(exploratory_only=True,peer_comparisons=0,platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),baseline_ref='4047596cd',old_sha256=hashlib.sha256(old.encode()).hexdigest(),candidate_sha256=hashlib.sha256(new.encode()).hexdigest(),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),cases=summary),indent=2)+'\n');print(json.dumps(summary,indent=2))
