from pathlib import Path
import subprocess,json,hashlib
p=Path(__file__).resolve().parent;rows=[];meta={}
harness='''
#[allow(dead_code)]enum ModelPosting{Standard(Vec<usize>),Bitmap(BitSet)}
fn show(initial:usize,position:Option<usize>,s:&BitSet){
 println!("{}",format!("{{\\\"initial_capacity\\\":{},\\\"inserted_position\\\":{},\\\"logical_capacity\\\":{},\\\"count\\\":{},\\\"buffer_len_bytes\\\":{},\\\"buffer_capacity_bytes\\\":{},\\\"bitset_bytes\\\":{},\\\"posting_model_bytes\\\":{}}}",initial,position.map(|p|p.to_string()).unwrap_or("null".into()),s.capacity,s.count(),s.bits.len()*8,s.bits.capacity()*8,std::mem::size_of::<BitSet>(),std::mem::size_of::<ModelPosting>()));
}
fn main(){for cap in [0,1,63,64,65,1024,65537]{let mut s=BitSet::new(cap);show(cap,None,&s);for pos in [0,63,64,127,128,65536,100003]{s.insert(pos);show(cap,Some(pos),&s);}}}
'''
for role in ['baseline','candidate']:
 s=(p/f'{role}-index.rs').read_text();a=s.index(('#[derive(Clone, PartialEq, Eq)]'if role=='candidate'else'#[derive(Debug, Clone, PartialEq, Eq)]')+'\npub struct BitSet');b=s.index('/// Pick one seek',a);source=s[a:b]+(harness.replace('s.capacity,','s.logical_capacity(),')if role=='candidate'else harness);f=p/f'layout-{role}.rs';f.write_text(source);binary=f'/tmp/lin-bitset-packed-layout-{role}'
 with(p/f'layout-{role}-build.log').open('w')as log:r=subprocess.run(['rustc','--edition=2024','-C','opt-level=3',str(f),'-o',binary],stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==0;data=[json.loads(l)for l in subprocess.check_output([binary],text=True).splitlines()];assert len(data)==56;rows.append(dict(role=role,observations=data));meta[role]=dict(source_sha256=hashlib.sha256(source.encode()).hexdigest(),binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest())
for a,b in zip(rows[0]['observations'],rows[1]['observations']):
 assert a['logical_capacity']==b['logical_capacity']and a['count']==b['count'];assert a['buffer_len_bytes']==b['buffer_len_bytes']and a['buffer_capacity_bytes']==b['buffer_capacity_bytes'];assert a['bitset_bytes']==b['bitset_bytes']==32 and a['posting_model_bytes']==b['posting_model_bytes']==32
(p/'layout.json').write_text(json.dumps(dict(variants=rows,provenance=meta,rss_measured=False),indent=2)+'\n');print('112 layout observations verified: BitSet32/32, posting model32/32, logical capacities/counts and Vec lengths/capacities equal')
