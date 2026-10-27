from pathlib import Path
p=Path(__file__).resolve().parent
exec=Path('src/exec.rs').read_text();persist=Path('src/persist.rs').read_text();store=Path('src/store.rs').read_text();lib=Path('src/lib.rs').read_text()
def replace_once(s,a,b):
 assert s.count(a)==1,(a,s.count(a));return s.replace(a,b)
exec=replace_once(exec,'    ) -> Result<(StmtOut, Option<crate::persist::PreparedEncoding>), Error> {','    ) -> Result<(StmtOut, Option<crate::persist::PreparedEncoding>), Error> {\n        let _profile = crate::mutation_profile::phase(0);')
exec=replace_once(exec,'    fn check_quotas(&self) -> Result<(), Error> {','    fn check_quotas(&self) -> Result<(), Error> {\n        let _profile = crate::mutation_profile::phase(1);')
exec=replace_once(exec,'    fn rollback(&mut self, undo: Undo, cat_backup: Option<Catalog>) {','    fn rollback(&mut self, undo: Undo, cat_backup: Option<Catalog>) {\n        let _profile = crate::mutation_profile::phase(7);')
persist=replace_once(persist,'    rmp_serde::encode::write_named(buf, rec).map_err(io_err)?;', '    {\n        let _profile = crate::mutation_profile::phase(2);\n        rmp_serde::encode::write_named(buf, rec).map_err(io_err)?;\n    }\n    crate::mutation_profile::payload(buf.len());')
a='''    let codec = magic[3];
    let mut checksum = crate::crc::Hasher::new();
    checksum.update(&[codec]);
    checksum.update(payload);
    let checksum = checksum.finalize().to_le_bytes();

    write_frame_bytes(log, codec, payload, checksum, len as u32).map_err(io_err)?;'''
b='''    let codec = magic[3];
    let checksum = {
        let _profile = crate::mutation_profile::phase(3);
        let mut checksum = crate::crc::Hasher::new();
        checksum.update(&[codec]);
        checksum.update(payload);
        checksum.finalize().to_le_bytes()
    };
    {
        let _profile = crate::mutation_profile::phase(4);
        write_frame_bytes(log, codec, payload, checksum, len as u32).map_err(io_err)?;
    }'''
persist=replace_once(persist,a,b);persist=replace_once(persist,'pub(crate) fn durable_sync(file: &File) -> io::Result<()> {','pub(crate) fn durable_sync(file: &File) -> io::Result<()> {\n    let _profile = crate::mutation_profile::phase(5);')
store=replace_once(store,'    pub fn write_snapshot(&mut self, persist: &mut Persist) -> Result<(), Error> {','    pub fn write_snapshot(&mut self, persist: &mut Persist) -> Result<(), Error> {\n        let _profile = crate::mutation_profile::phase(6);')
lib+='\n#[doc(hidden)]\npub mod mutation_profile;\n'
for name,s in [('exec',exec),('persist',persist),('store',store),('lib',lib)]: (p/f'instrumented-{name}.rs').write_text(s)
s=(p.parent/'2026-10-28-cow-vnext-full-engine'/'compare.rs').read_text();(p/'original-compare.rs').write_text(s)
s=replace_once(s,'fn main() -> airbug_bench::Result<()> {','fn main() -> airbug_bench::Result<()> {\n    lin::mutation_profile::configure();')
s=replace_once(s,'    run_suite(suite, &args)\n}', '    let result = run_suite(suite, &args);\n    lin::mutation_profile::flush();\n    result\n}')
a='''    match op { DurableOp::Update => write_update_lin(w), DurableOp::Delete => write_delete_lin(w), DurableOp::Commit => tx_update_commit_lin(w), DurableOp::Rollback => tx_update_rollback_lin(w) }'''
b='''    lin::mutation_profile::start();
    let affected = match op { DurableOp::Update => write_update_lin(w), DurableOp::Delete => write_delete_lin(w), DurableOp::Commit => tx_update_commit_lin(w), DurableOp::Rollback => tx_update_rollback_lin(w) };
    lin::mutation_profile::finish();
    affected'''
s=replace_once(s,a,b)
a='    state.db.get_mut().as_mut().unwrap().run_prepared(&state.update).unwrap().done.n'
b='    lin::mutation_profile::start();\n    let affected = state.db.get_mut().as_mut().unwrap().run_prepared(&state.update).unwrap().done.n;\n    lin::mutation_profile::finish();\n    affected'
s=replace_once(s,a,b);(p/'instrumented-compare.rs').write_text(s)
print('Prepared exact scoped phase hooks; no production files changed')
