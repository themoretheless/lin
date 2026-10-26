# Small-batch maintenance/WAL overlap after NEON — CPU feasibility

2026-10-26, frozen source aa8f307f. Current classifier and NEON/parallel writer helper block copied byte-for-byte and hashed. No production edits for this CPU experiment. Default public Db fixtures with real embeddings and the existing [wing,ts] index. This is a new composition using the accepted NEON writer and an explicit per-operation cold-worker control; it does not accept the earlier rejected scoped-thread prototype or pool old CPU series.

Four rotated modes: serial A,identical serial B,warm persistent worker,cold newly created worker. Two six-process series,24 rounds per case:4608 timed compositions. Input Row Vec cloning and fresh Db/index setup outside timing; timer includes vector-reference preparation,output allocation,Arc<Vec<Row>> transfer,queue/reply/wait,maintenance and ownership recovery. Cold mode additionally includes Worker::new for each operation. Join/shutdown,output destruction,Store publication and all verification are outside timing. A separate warm worker remains alive/idle during cold samples. No own builds/tests overlap measurement; all12 processes exit0 and hashes remain unchanged.

Every sample compares exact vector bytes,all public row values plus embedding f32 bits,every ID-map entry,and independently expected FTS/filter ID sets after timing. Caller Arc::try_unwrap checks worker release before reply. Deterministic rejected-channel checks retain ownership, but CPU failure coverage is not a full-engine transaction/OS resource-exhaustion campaign.

| Case | Warm gain series1 /2 | Cold gain series1 /2 | Positive processes in each worker series | A/A absolute difference series1 /2 |
|---|---:|---:|---:|---:|
|512 | +40.551% /+40.847% | +35.462% /+34.152% |6/6 |1.130% /0.634% |
|1k | +43.481% /+44.829% | +39.601% /+40.468% |6/6 |1.223% /3.243% |
|10k composition control | +28.838% /+31.025% | +28.748% /+30.925% |6/6 |0.748% /1.022% |
|127 serial guard | +0.143% /+0.017% | +0.447% /−0.091% | mixed |0.341% /0.197% |

The10k CPU serial comparison is not an additional production improvement: production already overlaps eligible large batches. Cold1k median composition286/300µs versus serial479/499µs; these exclude full column packing/scalar payload/framing/checksum/file write/Full sync/transaction paths and final Store publication. Worker constructor here uses standard thread spawn and a simple job; the production weak lease registry,named worker and full logical Pack cost require the engine gate.

## Decision

Proceed to full-engine qualification of extending the existing worker to512..4095 rows with at least256Ki finalized embedding floats. Preserve the old2Mi minimum for4096..16384; do not accidentally enable large low-dimensional batches. Explicit transactions,multi-statements,edges,native and other bounds retain existing behavior. The CPU model is not production acceptance; require two native/Full durable process-pair series,interleaved A/A/B,actual511/512 boundary,byte-equal WAL/process-exit replay and resources before retention. No superiority claim versus peers is made here.
