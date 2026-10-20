-- Executable SQL used by the research test.
DO $test$
BEGIN
    PERFORM 31;
    IF false THEN
        RAISE EXCEPTION 'The untaken test arm executed';
    ELSE
        PERFORM 32;
    END IF;
END
$test$;
