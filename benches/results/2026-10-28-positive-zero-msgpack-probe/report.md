# Positive-zero MessagePack primitive compatibility probe

This is an isolated serializer probe, **not an engine change or a timing benchmark**. The current experimental runtime and frozen production source04749994b9ca7a090476b95401f0f7166d8b870a both decoded all28 Cell-vector cases to the exact original f32 bit patterns. The same probe source was compiled against each library; the old snapshot's source hashes were checked before/after. Current runtime sources were untouched, and temporary examples were removed.

The probe emits only positive zero (to_bits()==0) as MessagePack integer0. All other f32 values retain ordinary f32 encoding. Cases include empty/small/768/1024-element vectors; positive and negative zero; positive/negative subnormals; minimum normals; infinities; quiet/signaling/signed NaN payloads. The existing encoder emits each f32 in5bytes. Integer0 costs1byte, and all28 cases verified the exact canonical-size adjustment:4bytes per positive zero. Vectors without positive zeros remain byte-identical.

A768-element vector with704positive zeros serializes as a Cell in1036bytes versus3852canonical bytes. This is a constructed sparse-vector example, not a measured full WAL record or current engine workload distribution. A compiled runtime negative using numeric equality (v==0) incorrectly normalizes negative zero and is rejected by the bitwise oracle with exit101.

This establishes compatibility only for the Cell decoder primitive on the two tested library sources. It does not qualify full old-reader WAL replay, frame CRC, generation/next_id,16MiB canonical quota behavior, failures, durability or timing. The program's legacy_reader_qualified=false refers to full WAL qualification; legacy-status.json separately records the28old Cell-decoder cases.

A possible future Full-only WAL serializer must retain Normal bytes exactly, preserve all IEEE bits and error/rollback behavior, enforce the original canonical record limit before any write, pass full legacy-reader frame/replay tests and a new frozen paired matrix. Smaller payload alone is insufficient. The pending FTS experiment remains separate; no optimizations are combined here.
