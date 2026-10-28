# Parallel builtin HashingEmbedder batch candidate

The independently audited COW bulk profile measured embedding32.286%/40.666%ofcallerwallat1k/10k. This candidate changes only embed.rs on topofCOW+CASa7d4b02c3;rejected index,SoA,bulk-template andDELETE-WAL changes are excluded.

Only builtin HashingEmbedder batches qualify:atleast4096texts,atleast4096×768densefloats,andavailable_parallelism>1. The right half runs inone scoped worker whilethecaller fills theleft;results concatenate inoriginal order. Eachthread retains itsownexisting scratch/normalization algorithm. Failedworkercreation fallsback toserial foralltexts beforeanyslab assignment. Small batches/oneCPU remainserial. Custom Embedder callbacks andsingle-text embed are unchanged. No workmoves toprepare time;vector dimensions/payload/order/floatbits remainpartofexecution.

Threefocused release tests pass:4097mixedASCII/Unicode/empty texts atdimensions8/31/768/1024matchserialandsingle output bits;forcedspawnfailure at0/1/4095/4096/4097preserveslength/order/bits;concurrent batches andchanging dimensions preserveTLSscratchisolation. Full release suite andfreshpaired ordinary CPU/bulk/durable/read qualification remainpending. UsingasecondCPU maychangeCPU/memory cost;this source isnotadopted andnospeedup isclaimeduntilmeasurement.

Final candidate949934fd8 preserves the original small-batch serial body verbatim (serial-path-proof.json) beside the parallel guard. Both3focused tests andthecomplete452release tests/2ignored pass. The initial serial-helper refactor andits prior full test log are retained separately. Final paired performance qualification remains pending.
