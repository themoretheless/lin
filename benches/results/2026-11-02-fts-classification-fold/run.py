from pathlib import Path
import hashlib,json,platform,statistics,subprocess,sys
p=Path(__file__).resolve().parent;root=p.parents[2];sha=lambda b:hashlib.sha256(b).hexdigest();assert not(p/'raw.json').exists(),'preserve previous attempt'
source=subprocess.check_output(['git','show','60e64559e:src/fts.rs'],cwd=root,text=True);a=source.index("pub(crate) fn lowercased<'a>");b=source.index('\n}\n',a)+2;old=source[a:b];new=old
start=new.index('    if text.is_ascii()');end=new.index('        scratch.clear();',start)
new=new[:start]+'''    let class = text.bytes().fold(0u8, |flags, byte| {
        flags | (byte >> 7) | (u8::from(byte.is_ascii_uppercase()) << 1)
    });
    if class == 0 {
        return text;
    }
    if class == 2 {
'''+new[end:]
program='use std::{hint::black_box,time::Instant};\n'
for name,body in [('baseline',old),('candidate',new)]:program+='#[inline(never)]\n'+body.replace('pub(crate) fn lowercased','fn '+name)+'\n'
program+=r'''
fn main(){let args:Vec<_>=std::env::args().collect();let mode=&args[1];let case=&args[2];let f:for<'a>fn(&'a str,&'a mut String)->&'a str=match mode.as_str(){"base"|"cont"=>baseline,"cand"=>candidate,_=>panic!()};let(text,calls)=match case.as_str(){"short_lower"=>("doc 99999 wal note".to_string(),2000000),"body_lower"=>("body 99999".to_string(),2000000),"long_lower"=>("lowercase words 123 ".repeat(64),100000),"short_upper"=>("DOC 99999 WAL NOTE".to_string(),2000000),"late_upper"=>(format!("{}Z","lower words ".repeat(64)),100000),"unicode_lower"=>("ёж é 🦔 ".repeat(16),100000),"unicode_upper"=>("ЁЖ İ ΣΟΣ ".repeat(16),100000),_=>panic!()};let mut scratch=String::new();let mut reference=String::new();for n in 0..=257{for marker in ["A","Z","@","[","İ","Σ","Ё","é","🦔"]{for pos in [0,n/2,n]{let input=format!("{}{}{}","x".repeat(pos),marker,"y".repeat(n-pos));assert_eq!(candidate(&input,&mut scratch),baseline(&input,&mut reference));}}}for code in 0..128{let input=char::from_u32(code).unwrap().to_string();assert_eq!(candidate(&input,&mut scratch),baseline(&input,&mut reference));}let expected:String=text.chars().flat_map(char::to_lowercase).collect();assert_eq!(f(&text,&mut scratch),expected);let start=Instant::now();for _ in 0..calls{black_box(f(black_box(&text),black_box(&mut scratch)));}let elapsed=start.elapsed().as_nanos();assert_eq!(f(&text,&mut scratch),expected);println!("{} {}",elapsed,calls);}
'''
(p/'micro.rs').write_text(program);subprocess.run(['rustc','-O','--edition=2024',str(p/'micro.rs'),'-o',str(p/'micro')],cwd=root,check=True)
sys.path.insert(0,str(root/'scripts'));from benchmark_role_balance import role_order,validate_orders
cases=['short_lower','body_lower','long_lower','short_upper','late_upper','unicode_lower','unicode_upper'];roles=['base','cont','cand'];orders={c:[role_order(r,i,roles)for r in range(1,7)]for i,c in enumerate(cases)};validate_orders(orders,roles);raw=[]
for series in [1,2]:
 for rnd in range(1,7):
  for case in cases:
   for role in orders[case][rnd-1]:
    cmd=[str(p/'micro'),role,case];ns,calls=map(int,subprocess.check_output(cmd,cwd=root,text=True).split());assert ns>0 and calls in [100000,2000000];raw.append(dict(series=series,round=rnd,case=case,role=role,elapsed_ns=ns,calls=calls,ns_per_call=ns/calls,command=cmd));(p/'process-status.json').write_text(json.dumps(raw,indent=2)+'\n')
summary={}
for series in [1,2]:
 for case in cases:
  gains=[];aa=[]
  for rnd in range(1,7):
   vals={o['role']:o['ns_per_call']for o in raw if o['series']==series and o['round']==rnd and o['case']==case};gains.append((1-vals['cand']/vals['base'])*100);aa.append(abs(vals['cont']/vals['base']-1)*100)
  summary[f'{series}-{case}']=dict(median_gain_percent=statistics.median(gains),median_aa_percent=statistics.median(aa),positive_pairs=sum(g>0 for g in gains),gains_percent=gains)
(p/'raw.json').write_text(json.dumps(raw,indent=2)+'\n');(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');(p/'status.json').write_text(json.dumps(dict(complete=True,processes=252,component_only=True,performance_qualified=False,peer_comparisons=0,runtime_source_unchanged=True,runtime_source_sha256=sha((root/'src/fts.rs').read_bytes()),source_reference='60e64559e',baseline_function_sha256=sha(old.encode()),candidate_function_sha256=sha(new.encode()),standalone_source_sha256=sha(program.encode()),binary_sha256=sha((p/'micro').read_bytes()),compiler=subprocess.check_output(['rustc','-Vv'],text=True),platform=platform.platform()),indent=2)+'\n');print(json.dumps(summary,indent=2))
