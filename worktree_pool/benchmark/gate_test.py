#!/usr/bin/env python3
"""Worked acceptance examples at the benchmark report boundary."""

import unittest

from worktree_pool.benchmark.evaluate import assess


def favorable_pairs():
    pairs = []
    for workload in (
        "unchanged",
        "small_edit",
        "revision_change",
        "revision_return",
        "two_callers",
    ):
        for batch in (1, 2):
            for index in range(10):
                pairs.append(
                    {
                        "workload": workload,
                        "batch": batch,
                        "pair": index,
                        "pooled": {
                            "total_ns": 80,
                            "callers": [{"total_ns": 70}, {"total_ns": 80}]
                            if workload == "two_callers"
                            else [{"total_ns": 80}],
                        },
                        "disposable": {
                            "total_ns": 100,
                            "callers": [{"total_ns": 90}, {"total_ns": 100}]
                            if workload == "two_callers"
                            else [{"total_ns": 100}],
                        },
                    }
                )
    return pairs


class GateTest(unittest.TestCase):
    def test_incomplete_evidence_holds_before_performance_failure(self):
        result = assess([], valid=True, accepted_costs=False)
        self.assertEqual(result["decision"], "hold")
        self.assertIn("incomplete", result["reasons"])

    def test_complete_gains_require_explicit_resource_acceptance(self):
        pairs = favorable_pairs()
        self.assertEqual(assess(pairs, valid=True)["decision"], "hold")
        self.assertEqual(
            assess(pairs, valid=True, accepted_costs=True)["decision"], "go"
        )
        self.assertEqual(
            assess(pairs, valid=True, accepted_costs=False)["decision"], "no-go"
        )

    def test_batch_disagreement_holds_before_consistent_failure_elsewhere(self):
        pairs = favorable_pairs()
        for pair in pairs:
            if pair["workload"] == "unchanged" and pair["batch"] == 2:
                pair["pooled"]["total_ns"] = 120
            if pair["workload"] == "small_edit":
                pair["pooled"]["total_ns"] = 120
        result = assess(pairs, valid=True, accepted_costs=True)
        self.assertEqual(result["decision"], "hold")
        self.assertIn("batch_disagreement", result["reasons"])

    def test_per_caller_regression_rejects_an_improved_two_caller_total(self):
        pairs = favorable_pairs()
        for pair in pairs:
            if pair["workload"] == "two_callers":
                pair["pooled"]["callers"][0]["total_ns"] = 110
        result = assess(pairs, valid=True, accepted_costs=True)
        self.assertEqual(result["decision"], "no-go")
        self.assertIn("secondary_regression", result["reasons"])
        self.assertEqual(
            assess(pairs, valid=False, accepted_costs=True)["decision"], "hold"
        )

    def test_ties_are_not_the_eighth_win(self):
        pairs = favorable_pairs()
        for pair in pairs:
            if pair["workload"] == "unchanged" and pair["pair"] >= 7:
                pair["pooled"]["total_ns"] = 100
        self.assertEqual(
            assess(pairs, valid=True, accepted_costs=True)["decision"], "no-go"
        )

    def test_secondary_batch_disagreement_holds_over_another_regression(self):
        pairs = favorable_pairs()
        for pair in pairs:
            if pair["workload"] == "revision_change" and pair["batch"] == 2:
                pair["pooled"]["total_ns"] = 120
            if pair["workload"] == "revision_return":
                pair["pooled"]["total_ns"] = 120
        self.assertEqual(
            assess(pairs, valid=True, accepted_costs=True)["decision"], "hold"
        )

    def test_nonpositive_duration_is_invalid_evidence(self):
        pairs = favorable_pairs()
        pairs[0]["pooled"]["total_ns"] = 0
        result = assess(pairs, valid=True, accepted_costs=True)
        self.assertEqual(result["decision"], "hold")
        self.assertIn("invalid_measurements", result["reasons"])


if __name__ == "__main__":
    unittest.main()
