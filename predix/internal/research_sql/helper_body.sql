-- Executable SQL helper used by both applications.
DO $helper$
BEGIN
    IF false THEN
        INSERT INTO research_values VALUES (91, 'untaken_helper');
    ELSE
        INSERT INTO research_values VALUES (12, 'helper');
    END IF;
END
$helper$;
