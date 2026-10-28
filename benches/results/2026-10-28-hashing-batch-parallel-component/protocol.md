# Large builtin hashing batch parallel component

Candidate949934fd8 versus COW+CAS baselinea7d4b02c3;only embed.rs changes runtime. Existing serial batch body is retained verbatim belowtheguard. Parallel execution requires>=4096texts,>=4096×768densefloats and>1Rust-visibleCPU. No workmoves toprepare. Custom backend callbacks/single-text vectors,dimensions/normalization/logicalpayload andqueryfixtures remainunchanged. Serial fallback handles failedworkercreation;joinpreserves order.

Target:existing Lin bulk INSERT10k. Eightcontrols:Lin bulk INSERT1k,point_get,filter_eq,UPDATE1k,scalar Full/Normal UPDATE,SQLitebulk INSERT10k andDuckDBfilter_eq. Same frozen compareharness andordinary uninstrumented release binaries. Two six-pair series,rotatingcases/roles,baseline/control share one executable,eightwallobservations,max_iterations1,zero warmup. All324fresh processes/2592observations retained. Both target windows need6/6positive pairs andmedian gain above median absolute A/A;everycontrol needs gain>=-max(2%,A/A). No trimming,thresholdrelaxation orunchanged retries for a pass.

The targethost must report>1availableCPU throughRust itself;otherwise itfails asunsupported andcannotcount as a parallel win. Capture kernel/filesystem;do nottune either. CPU/memory workmayincrease whilewall decreases;noRSS orCPU-efficiency claim follows fromlatency alone.

Threefocused tests verify bitwise order atmultiple dimensions,empty/Unicode input,concurrent scratchisolation andforcedspawnfailure. Full release suite andruntime reversed-output mutant are rerun inLinux,followed byall18freshfull-value smokes. Theprofile indicatingembedding32–41%isprioritization evidenceonly. Component successcannotqualifythefullsource oreight-peer objective;everyfailedpair andallsource/build/test/smoke/hostmetadata must be retained.

GNUtime-vrecords CPU/RSS aroundeachwholefreshprocessinallroles. These resources includesetup/readback/destruction andarenot isolated operationCPU/RSS. Resourcefilehashes are attached toeachprocessrecord;thechild benchmark retainsitsoriginaltimeboundary. No efficiencyclaim is inferredfromwallgainalone.
