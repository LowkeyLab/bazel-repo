-- The inventory must not evaluate this disconnected source.
SELECT 1 / 0
AS disconnected_failure;
