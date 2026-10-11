"""Shared declared-input provider consumed by generation and runtime validation."""

CargoPublishInfo = provider(
    doc = "Declared package tree and versioned generation contract for validation.",
    fields = {"package": "Staged tree artifact", "contract": "Generation contract JSON"},
)
