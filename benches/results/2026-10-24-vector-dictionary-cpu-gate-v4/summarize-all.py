from pathlib import Path
import json,statistics,hashlib,subprocess
root=Path(__file__).resolve().parent.parent
names=['2026-10-24-vector-dictionary-cpu-gate']+[f'2026-10-24-vector-dictionary-cpu-gate-v{i}' for i in [2,3,4]]
combined={}
for version,name in enumerate(names,1):
 p=root/name;meta=json.loads((p/'provenance.json').read_text())
 assert all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in meta['source_sha256'].items())
 assert json.loads((p/'build-status.json').read_text())['exit_code']==0
 assert len(json.loads((p/'status.json').read_text()))==6 and not any(json.loads((p/'status.json').read_text()).values())
 by_case={}
 for i in range(1,7):
  docs=[json.loads(x) for x in (p/f'process-{i}.jsonl').read_text().splitlines()];assert len(docs)==4
  for d in docs:
   assert d['exact_bits_roundtrip'] and len(d['sparse_ns'])==len(d['candidate_ns'])==12
   by_case.setdefault(d['case'],[]).append(d)
 summary={};rows=[]
 for case,docs in by_case.items():
  a=[statistics.median(d['sparse_ns']) for d in docs];b=[statistics.median(d['candidate_ns']) for d in docs]
  sizes={(d['sparse_bytes'],d['candidate_bytes'],d['dictionary_selected']) for d in docs};assert len(sizes)==1
  old_bytes,new_bytes,selected=sizes.pop()
  item={'reference_process_medians_ns':a,'candidate_process_medians_ns':b,'reference_median_ns':statistics.median(a),'candidate_median_ns':statistics.median(b),'paired_gain_percent':statistics.median((x-y)/x*100 for x,y in zip(a,b)),'wins':sum(y<x for x,y in zip(a,b)),'reference_bytes':old_bytes,'candidate_bytes':new_bytes,'dictionary_selected':selected}
  summary[case]=item
  rows.append(f'| {case} | {item["reference_median_ns"]/1e6:.6f} | {item["candidate_median_ns"]/1e6:.6f} | {item["paired_gain_percent"]:.3f}% | {item["wins"]}/6 | {old_bytes} | {new_bytes} | {selected} |')
 combined[str(version)]=summary;(p/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 report=f'''# Vector dictionary CPU gate v{version}

Production source `{meta['source_commit']}`; core sources unchanged and hashes verified after all runs. Diagnostic Rust source gate.rs, exact current sparse reference, build/status logs, pinned binary hash, six process JSONL outputs retained. Temporary Cargo example removed immediately after build. No production codec, reader, WAL file or runtime setting changed. No workspace rerun: this is a diagnostic executable, not a shipped engine patch.

## Scope and method

Real default 768-dimension embeddings created by Db::empty insertion using the Rust compare fixture (title/body/id/uri/wing/layer, wing+ts index, fixed timestamps), then Arc-owned immutable vectors retained for CPU trials. Setup and model work excluded. Six independent release processes; twelve alternating old/new function rounds per case/process, buffer capacities reserved outside timing and buffers reused/cleared. First-touch sample remains included; no outliers removed. Per-process medians aggregated before paired percent gains. No competing tests/builds/benchmarks initiated by this runner overlap timing; host activity otherwise uncontrolled.

None/Some(empty), positive/negative zero, distinct quiet/signalling NaN bits, +/-Inf, partial blocks, dimensions 65535/65536/65537 and high-cardinality escape are bit-roundtripped, including forced dictionary mode and an existing output prefix. Decoder here is trusted-fixture/assert-based **not production untrusted-input code**. No fuzz/crash/reader compatibility safety claim.

Row proposal: old dimension/count headers, <=16 exact-bit dictionary values, u16 indexes at dimensions<=65536 (otherwise u32), u8 value ordinals, raw u32-index/u32-bits escape. Encoded dictionary selected only when beneficial; baseline code is mechanically copied from current persist.rs. This is component CPU/byte evidence, not full-frame size, sync speed, DB throughput, operation p99 or a peer win.

'''
 if version<4:
  report+='''Reference is the existing **serial sparse append** routine, excluding sparse classification, decoded-budget checks, fields/framing/checksum/sync. 10k here is serial kernel evidence, not current four-way production wall time. v1 uses variable-size 3/5-byte entry copies; v2 constant-size writes; v3 also probes the first four zero-initialized dictionary slots before searching the tail. The first three candidates have CPU penalties even though default vector bytes shrink.

'''
 else:
  report+='''Reference is current **classification plus chosen sparse/dense emission**, including existing four-way classifier/encoder at 10k (copied helpers and dense loop). Candidate only considers dictionary at 128..4095 rows with 64KiB..8MiB dense metadata size, samples up to four rows for <=16-value eligibility and requires >=20% vector-byte savings. It encodes once, choosing from computed byte costs, avoiding a separate full-column sparse-classification pass on the eligible 1k path. High-cardinality and 10k controls use the existing path. JSON retains legacy sparse_ns/sparse_bytes field names, but v4 times the whole vector representation choice.

Metadata/probe overhead is included. Outer column/frame metadata, decoded-budget enforcement, scalar columns, record checksums/write/sync and actual production reader are excluded; these gates remain. Do not subtract earlier profiler medians to infer this gain: both compared choices execute on the same frozen input in the same process here.

'''
 report+='''| Case | Reference ms | Candidate ms | Paired gain | Faster processes | Reference bytes | Candidate bytes | Dictionary selected |
|---|---:|---:|---:|---:|---:|---:|---|
'''+ '\n'.join(rows)+'\n'
 if version<4:
  report+='''
Decision: do not adopt this serial encoder alone. Every process loses CPU time on default vectors; high-cardinality fallback also wastes work before retaining legacy bytes. Preserve negative results. The v4 eligibility/whole-choice prototype is a separate experiment; it does not erase these costs.
'''
 else:
  report+='''
Decision: CPU/byte feasibility passes for the 1k whole-choice prototype (6/6 process gains). Default 1k vector bytes 238,992 ->112,542 (-52.91%). Historical/current full WAL payload 336,968 would project to 210,518 bytes if other fields/framing stayed unchanged (~37.5% reduction); this is arithmetic, not an actually written frame or latency result. Default 10k intentionally retains the existing parallel representation, not the slower serial dictionary kernel.

Next required milestone: versioned production writer/strict bounded decoder, raw/dense/mixed fallbacks, all old-codec golden inputs, old pinned reader rejecting new complete frames without truncation, malformed/checksum/64MiB-budget tests, durable reopen/checkpoint/replay/crash and full workspace. Then two independent six-pair end-to-end native/durable SQLite-controlled series plus resource controls. Do not enable new writes merely because this CPU gate passes. See ../2026-10-24-current-wal-phase-profile/dictionary-codec-plan.md. Durable 1k gap and MSSQL/Kusto remain unqualified; complete named-peer objective stays open.
'''
 (p/'report.md').write_text(report)
 for f in p.glob('*.log'):
  if f.stat().st_size:f.write_text(f.read_text().rstrip()+'\n')
(root/names[-1]/'all-variants.json').write_text(json.dumps(combined,indent=2)+'\n')
assert not subprocess.check_output(['git','diff','--','src','examples','Cargo.toml','Cargo.lock'])
print('four variant reports verified; production sources unchanged')
