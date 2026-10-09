#!/usr/bin/env python3
"""Evaluate retained paired measurements against the approved acceptance policy."""

WORKLOADS = (
    "unchanged",
    "small_edit",
    "revision_change",
    "revision_return",
    "two_callers",
)


def assess(pairs, *, valid, accepted_costs=None):
    """Return a gate assessment; absent evidence never implies success."""
    expected = {
        (workload, batch, index)
        for workload in WORKLOADS
        for batch in (1, 2)
        for index in range(10)
    }
    keys = [(pair["workload"], pair["batch"], pair["pair"]) for pair in pairs]
    if len(keys) != len(expected) or set(keys) != expected:
        return {"decision": "hold", "reasons": ["incomplete"]}
    try:
        durations = [
            arm["total_ns"]
            for pair in pairs
            for arm in (pair["pooled"], pair["disposable"])
        ]
        durations += [
            caller["total_ns"]
            for pair in pairs
            for arm in (pair["pooled"], pair["disposable"])
            for caller in arm["callers"]
        ]
        if any(type(duration) is not int or duration <= 0 for duration in durations):
            return {"decision": "hold", "reasons": ["invalid_measurements"]}
        if any(
            len(pair[arm]["callers"]) != (2 if pair["workload"] == "two_callers" else 1)
            for pair in pairs
            for arm in ("pooled", "disposable")
        ):
            return {"decision": "hold", "reasons": ["invalid_measurements"]}
    except (KeyError, TypeError):
        return {"decision": "hold", "reasons": ["invalid_measurements"]}
    if not valid:
        return {"decision": "hold", "reasons": ["invalid_conditions"]}
    counts = {}
    for workload in ("unchanged", "small_edit"):
        counts[workload] = [
            sum(
                pair["pooled"]["total_ns"] < pair["disposable"]["total_ns"]
                for pair in pairs
                if pair["workload"] == workload and pair["batch"] == batch
            )
            for batch in (1, 2)
        ]
    if any((wins[0] >= 8) != (wins[1] >= 8) for wins in counts.values()):
        return {
            "decision": "hold",
            "reasons": ["batch_disagreement"],
            "primary_wins": counts,
        }
    slowdowns = {}
    for workload in ("revision_change", "revision_return", "two_callers"):
        metrics = (
            ("total", "caller_0", "caller_1")
            if workload == "two_callers"
            else ("total",)
        )
        for metric in metrics:

            def duration(arm, selected_metric=metric):
                if selected_metric == "total":
                    return arm["total_ns"]
                return arm["callers"][int(selected_metric[-1])]["total_ns"]

            slowdowns[f"{workload}/{metric}"] = [
                sum(
                    duration(pair["pooled"]) > duration(pair["disposable"])
                    for pair in pairs
                    if pair["workload"] == workload and pair["batch"] == batch
                )
                for batch in (1, 2)
            ]
    if any((slows[0] >= 8) != (slows[1] >= 8) for slows in slowdowns.values()):
        return {
            "decision": "hold",
            "reasons": ["batch_disagreement"],
            "secondary_slowdowns": slowdowns,
        }
    if any(slows[0] >= 8 for slows in slowdowns.values()):
        return {
            "decision": "no-go",
            "reasons": ["secondary_regression"],
            "secondary_slowdowns": slowdowns,
        }
    if any(wins[0] < 8 for wins in counts.values()):
        return {
            "decision": "no-go",
            "reasons": ["primary_benefit_absent"],
            "primary_wins": counts,
        }
    if accepted_costs is None:
        return {"decision": "hold", "reasons": ["resource_acceptance_pending"]}
    if not accepted_costs:
        return {"decision": "no-go", "reasons": ["resource_costs_rejected"]}
    return {"decision": "go", "reasons": []}
