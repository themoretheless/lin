import tempfile
import unittest
from pathlib import Path
from benchmark_source_integrity import snapshot

class SourceIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root/'src').mkdir()
        (self.root/'src/lib.rs').write_text('// already dirty overlay\n')
        (self.root/'Cargo.toml').write_text('[package]\n')
        self.binary = self.root/'peer'
        self.binary.write_bytes(b'original executable')
        self.before = snapshot(self.root, self.binary)
    def test_generated_reports_and_python_cache_do_not_drift(self):
        d=self.root/'benches/results/new-run';d.mkdir(parents=True)
        (d/'run.json').write_text('{}')
        (d/'measure.py').write_text('# archived protocol')
        c=self.root/'scripts/__pycache__';c.mkdir(parents=True)
        (c/'helper.pyc').write_bytes(b'cache')
        self.assertEqual(self.before,snapshot(self.root,self.binary))
    def test_unchanged_dirty_overlay_is_stable(self):
        self.assertEqual(self.before,snapshot(self.root,self.binary))
    def test_source_edit_addition_and_deletion_each_drift(self):
        f=self.root/'src/lib.rs';f.write_text('// changed\n')
        self.assertNotEqual(self.before,snapshot(self.root,self.binary))
        f.write_text('// already dirty overlay\n');g=self.root/'src/new.rs';g.write_text('// new')
        self.assertNotEqual(self.before,snapshot(self.root,self.binary))
        g.unlink();f.unlink()
        self.assertNotEqual(self.before,snapshot(self.root,self.binary))
    def test_binary_replacement_drifts_and_missing_binary_fails(self):
        self.binary.write_bytes(b'replaced executable')
        self.assertNotEqual(self.before,snapshot(self.root,self.binary))
        self.binary.unlink()
        with self.assertRaises(FileNotFoundError):snapshot(self.root,self.binary)
    def test_build_harness_and_driver_changes_drift(self):
        for name in ['Cargo.lock','build.rs','rust-toolchain.toml','.cargo/config.toml','examples/peer_bench.rs','benches/compare.rs','scripts/bench-peers.py','lin-derive/Cargo.toml','lin-derive/src/lib.rs']:
            f=self.root/name;f.parent.mkdir(parents=True,exist_ok=True);f.write_text('input')
            self.assertNotEqual(self.before,snapshot(self.root,self.binary),name)
            f.unlink()
    def test_finish_records_real_drift_and_fails_closed(self):
        import importlib.util
        from types import SimpleNamespace
        from unittest.mock import patch
        spec=importlib.util.spec_from_file_location('peer_driver',Path(__file__).with_name('bench-peers.py'))
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        module.ROOT=self.root
        out=self.root/'reports';out.mkdir()
        args=SimpleNamespace(cases=[],engines=['lin'],repeats=1,output=out,require_wins=False,lin_binary=str(self.binary))
        def run():return dict(observations=[],errors=[],medians_ns={},source_identity_before=self.before,git_status_before='before')
        with patch.object(module,'report',return_value='report'),patch.object(module.subprocess,'check_output',return_value='after'):
            clean=run();self.assertEqual(module.finish(args,clean),0)
            self.assertTrue(clean['git_status_changed']);self.assertFalse(clean['source_drift'])
            (self.root/'src/lib.rs').write_text('// changed')
            drift=run();self.assertEqual(module.finish(args,drift),2)
            self.assertTrue(drift['source_drift']);self.assertEqual(drift['status'],'incomplete')
            self.binary.unlink()
            missing=run();self.assertEqual(module.finish(args,missing),2)
            self.assertTrue(missing['source_drift']);self.assertTrue(missing['errors'])
    def test_peer_only_snapshot_explicitly_has_no_lin_binary(self):
        self.assertIsNone(snapshot(self.root,None)['binary'])
if __name__=='__main__':unittest.main()
