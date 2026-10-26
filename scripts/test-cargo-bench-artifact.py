#!/usr/bin/env python3
import unittest
from cargo_bench_artifact import bench_executable
class Artifact(unittest.TestCase):
 def test_actual_linux_ansi_shape(self):
  text='\x1b[1m\x1b[92m  Executable\x1b[0m benches/compare.rs (target/release/deps/compare-0f624f91cce63f72)\n'
  self.assertEqual(bench_executable(text,'compare'),'target/release/deps/compare-0f624f91cce63f72')
 def test_plain_and_spaces(self):self.assertEqual(bench_executable('  Executable benches/compare.rs (/tmp/a b/compare)\n','compare'),'/tmp/a b/compare')
 def test_other_artifact_ignored(self):self.assertEqual(bench_executable('Executable benches/other.rs (other)\nExecutable benches/compare.rs (compare)\n','compare'),'compare')
 def test_missing_rejected(self):
  with self.assertRaises(ValueError):bench_executable('Finished successfully','compare')
 def test_ambiguous_rejected(self):
  with self.assertRaises(ValueError):bench_executable('Executable benches/compare.rs (a)\nExecutable benches/compare.rs (b)\n','compare')
if __name__=='__main__':unittest.main()
