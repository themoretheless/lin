# Single-pass extent replay

Read-only open previously validated the logical frame region in from_file, read/validated it again in replay_records, then decoded it again for application. This change separates strict header parsing from region replay: read-only open reads the header once and the logical region once, validates every checksummed frame while decoding, and applies only records newer than the snapshot. Existing writer open still validates the file before returning its handle; its later replay avoids an extra validation/decode pass. Explicit read_frames remains fully validated.

No framing, CRC, snapshot-generation or physical-length constraints are removed. In particular, snapshot-covered records still pass CRC/codec/frame decoding before filtering. The replay callback error behavior remains the legacy behavior: a failed open never returns a partially recovered Store.

Focused release suite passed: 10 tests, zero failures, one ignored child helper. The counting Read+Seek fixture verifies exactly HEADER_BYTES + logical WAL bytes are read, with no padding or second region read. Corrupt CRC in a snapshot-covered record remains an error. Every header/frame byte corruption, stale prepared metadata and all five actual SIGKILL stages pass again.

Full feature/default follow-ups remain pending. No measured open-latency or throughput gain is claimed. Linux run 41391257035 is pinned to earlier 20a408f6c and excludes this later source change. Experimental format growth, standalone export/bootstrap, power-loss qualification and the complete all-peer goal remain open.
