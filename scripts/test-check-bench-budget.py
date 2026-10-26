#!/usr/bin/env python3
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('budget', ROOT/'scripts/check-bench-budget.py')
budget = importlib.util.module_from_spec(spec)
spec.loader.exec_module(budget)


class Contract(unittest.TestCase):
    def setUp(self):
        self.baseline = json.loads((ROOT/'benches/ci-baseline.json').read_text())
        self.data = dict(status='complete', environment=dict(os='linux', arch='x86_64'), observations=[])
        for case, reference in self.baseline['cases'].items():
            for engine, multiplier in [('lin', .5), ('sqlite', 2), ('duckdb', 3)]:
                self.data['observations'].append(dict(case=case.rsplit('/',1)[0]+'/'+engine, metric='wall', availability=dict(state='available'), operations=4, value=str(reference['median_ns_per_op']*multiplier*4)))

    def test_valid_peers_and_normalized_operations(self):
        lines, failures = budget.evaluate(self.baseline, self.data)
        self.assertEqual(failures, 0)
        self.assertIn('historical only', '\n'.join(lines))
        self.assertIn('win 4.00x', '\n'.join(lines))

    def test_compatible_legacy_hard_limit_retained_even_with_peer_win(self):
        self.data['environment'] = dict(self.baseline['platform'])
        for o in self.data['observations']:
            o['value'] = str(float(o['value'])*8)
        self.assertGreater(budget.evaluate(self.baseline, self.data)[1], 0)

    def test_foreign_absolute_limit_does_not_override_peer_comparison(self):
        for o in self.data['observations']:
            o['value'] = str(float(o['value'])*8)
        self.assertEqual(budget.evaluate(self.baseline, self.data)[1], 0)

    def test_peer_loss_and_tie_fail_on_every_platform(self):
        for platform in [self.data['environment'], self.baseline['platform']]:
            for factor in [1, 2]:
                data = copy.deepcopy(self.data);data['environment'] = platform
                data['observations'][0]['value'] = str(float(data['observations'][1]['value'])*factor)
                self.assertGreater(budget.evaluate(self.baseline, data)[1], 0)

    def test_missing_required_comparison_fails(self):
        for index in [0, 1, 2]:
            data = copy.deepcopy(self.data);data['observations'].pop(index)
            self.assertGreater(budget.evaluate(self.baseline, data)[1], 0)

    def test_incomplete_and_invalid_data_rejected(self):
        for patch in [dict(status='incomplete'),dict(environment={})]:
            data = copy.deepcopy(self.data);data.update(patch)
            with self.assertRaises(ValueError):budget.evaluate(self.baseline, data)
        for key, values in [('operations',[0,-1,1.5,True,None]),('value',['NaN','Infinity','0','-1',True,None]),('availability',[{},dict(state='unavailable')])]:
            for value in values:
                data = copy.deepcopy(self.data);data['observations'][0][key] = value
                with self.assertRaises((ValueError, TypeError)):budget.evaluate(self.baseline, data)

    def test_bad_baseline_rejected(self):
        for patch in [dict(schema=1),dict(cases={}),dict(required_peers=[]),dict(required_peers=['sqlite','sqlite']),dict(platform={})]:
            baseline = copy.deepcopy(self.baseline);baseline.update(patch)
            with self.assertRaises(ValueError):budget.evaluate(baseline,self.data)


if __name__ == '__main__':unittest.main()
