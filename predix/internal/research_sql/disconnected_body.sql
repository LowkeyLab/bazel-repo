-- This anonymous body must remain disconnected.
DO $unused$
BEGIN
    RAISE EXCEPTION 'The disconnected body executed';
END
$unused$;
