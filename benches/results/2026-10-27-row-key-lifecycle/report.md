# Row-key lifecycle and wire compatibility diagnostic

Standalone candidate only; production Row/API/source unchanged. Key uses lossless InlineStr <=22 bytes, Arc<str> otherwise. Main205 Cell implementation unchanged. Two scenarios: build/drop a row, and build + two clones + patch existing field + lookup + drop all three rows. Unlike the earlier clone-only test, candidate key construction and every Drop are inside timing.

Six processes, six shapes, two scenarios, 24 rotated A/A/B samples, 512 operations per sample: 2654208 lifecycle operations. Counters disabled during timings; allocation requests measured separately. All raw data and exact binary/gate SHA retained. run.py rebuilds the summary without external benchmark dependencies.

| Shape/scenario | Median gain | Absolute A/A | Positive processes | Allocation requests |
|---|---:|---:|---:|---:|
| boundary9/build | 18.392% | 0.468% | 6/6 | 11 -> 6 |
| boundary9/clonepatch | 54.754% | 0.826% | 6/6 | 31 -> 8 |
| long9/build | -1.336% | 0.312% | 0/6 | 11 -> 11 |
| long9/clonepatch | 45.481% | 0.495% | 6/6 | 31 -> 13 |
| mixed32/build | 11.655% | 0.418% | 6/6 | 37 -> 21 |
| mixed32/clonepatch | 46.289% | 0.483% | 6/6 | 109 -> 29 |
| short32/build | 35.305% | 0.386% | 6/6 | 37 -> 5 |
| short32/clonepatch | 55.433% | 0.563% | 6/6 | 109 -> 13 |
| short9/build | 40.939% | 0.191% | 6/6 | 11 -> 2 |
| short9/clonepatch | 62.625% | 0.184% | 6/6 | 31 -> 4 |
| unicode9/build | 43.224% | 0.250% | 6/6 | 11 -> 2 |
| unicode9/clonepatch | 62.564% | 0.179% | 6/6 | 31 -> 4 |

Explicit negative control: long9 build regresses 1.336%, 0/6 positive, beyond 0.312% A/A. Long-name Arc creation has a real cost in this kernel. It cannot be hidden behind positive clone results. Mixed32 construction remains positive in this fixture, but no general no-regression claim follows.

Exact serde oracle outside timing: JSON and MessagePack serialization bytes match the String-key reference; MessagePack bytes from both representations decode into both maps and all values match. JSON reference decodes into the candidate. Shapes include 22/23-byte boundary names and Unicode names; keys are not truncated. These checks do not establish arbitrary invalid-input behavior, complete schema compatibility, a full WAL frame or historical reader compatibility.

Integration remains open: public Row is a type alias for BTreeMap<String,Cell>, so changing its key type silently breaks callers with explicit map types, owned iterators and entries. Need an API compatibility design and measure conversion costs at real engine boundaries, not claim standalone lifecycle wins as full UPDATE acceleration. Full engine native/durable/read/RSS/peer controls remain required. No production optimization accepted by these results.
