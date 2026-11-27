import sys
import ctypes as c,ctypes.util,subprocess,json,tempfile,pathlib,time,statistics as st,os,random
lib=c.CDLL(ctypes.util.find_library('lz4') or 'liblz4.so.1')
report={}
rng=random.Random(891)
for n in (1000,10000):
 with tempfile.TemporaryDirectory() as d:
  p=pathlib.Path(d)/'capture'
  run=subprocess.run([sys.argv[1]],input=json.dumps(dict(insert_rows=n,samples=1,timestamp_ms=123456,durable_dir=d)),text=True,capture_output=True,check=True,env=dict(os.environ,LIN_CAPTURE_WAL_PAYLOAD=str(p)))
  src=p.read_bytes()
  assert len(src)+13==int(run.stderr.split('frame_profile_begin')[1].split('frame_cost ')[1].split()[0])
  out=c.create_string_buffer(lib.LZ4_compressBound(len(src)));dst=c.create_string_buffer(len(src))
  records=[]
  for trial in range(24):
   modes=[0,1,4,8,16];rng.shuffle(modes)
   trial_data={}
   for mode in modes:
    with tempfile.TemporaryFile(dir=d,buffering=0) as f:
     start=time.perf_counter_ns()
     size=lib.LZ4_compress_fast(src,out,len(src),len(out),mode) if mode else len(src)
     buf=memoryview(out).cast('B')[:size] if mode else src
     compressed=time.perf_counter_ns()
     assert f.write(buf)==size
     os.fdatasync(f.fileno())
     end=time.perf_counter_ns()
     if mode: assert size>0 and lib.LZ4_decompress_safe(out,dst,size,len(dst))==len(src) and dst.raw==src
     trial_data[mode]=(size,compressed-start,end-start)
   records.append(trial_data)
  for mode in (0,1,4,8,16):
   delta=[(t[mode][2]-t[0][2])/1e6 for t in records]
   print(n,mode,records[0][mode][0], round(st.median(t[mode][1] for t in records)/1e6,3),round(st.median(delta),3), sum(v<0 for v in delta))
  report[n]=records
  pathlib.Path(sys.argv[2]).write_text(json.dumps(report))
