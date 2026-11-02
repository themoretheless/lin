from pathlib import Path
import hashlib,json,tarfile
p=Path(__file__).resolve().parent;sha=lambda b:hashlib.sha256(b).hexdigest()
assert (p/'independent-native-audit.json').is_file()
summary=json.loads((p/'native-summary.json').read_text())
lines=['# Constant modulus: local native insertion discovery','','Full candidate default463/2 and extent-feature483/4 suites passed. Expanded independent dense embedding oracle compares output bits. Same root peer_bench example, compiler and default+experimental-extent-wal features as the immutable baseline. Only src/embed.rs differs in the complete build/source closure. Worker timers, quotas, embeddings and FTS remain unchanged. Two balanced series:108 fresh processes,864 samples,3996000 exact six-field row readbacks independently audited. Local host activity uncontrolled.','','| Series | Rows | Gain % | AA % | Positive pairs |','|---|---:|---:|---:|---:|']
for key,d in summary.items():
 s,n=key.split('-');lines.append(f"| {s} | {n} | {d['median_gain_percent']:+.3f} | {d['median_aa_percent']:.3f} | {d['positive_pairs']}/6 |")
lines+=['','Discovery only. Every loss and noisy window is retained. No full-source performance gate or external-peer superiority follows. The earlier full COW/extent acceptance gate remains failed; main is untouched. Linux qualification is separate.']
(p/'native-report.md').write_text('\n'.join(lines)+'\n')
archive=p/'native-raw.tar.gz';assert not archive.exists()
files=sorted((p/'native-raw').iterdir());manifest={f.name:sha(f.read_bytes())for f in files};assert len(files)==216
with tarfile.open(archive,'w:gz')as tar:
 for f in files:tar.add(f,arcname=f.name,recursive=False)
with tarfile.open(archive)as tar:
 assert {m.name:sha(tar.extractfile(m).read())for m in tar.getmembers()}==manifest
(p/'native-raw-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
(p/'archive-audit.json').write_text(json.dumps(dict(raw_members=len(files),all_member_hashes_verified=True,archive_sha256=sha(archive.read_bytes()),adopted=False,full_objective_achieved=False),indent=2)+'\n')
print((p/'native-report.md').read_text())
