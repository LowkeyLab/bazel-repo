-- Bounded original SQL and anonymous procedural fixture.
CREATE TABLE research_values (
    value INTEGER NOT NULL,
    origin TEXT NOT NULL
);

INSERT INTO research_values (
    value, origin
) VALUES (7, 'ordinary');

SELECT value
FROM research_values
WHERE origin = 'ordinary';

DO $research$
BEGIN
    IF false THEN
        INSERT INTO research_values (value, origin)
        VALUES (99, 'untaken');
    ELSE
        INSERT INTO research_values (value, origin)
        VALUES (11, 'anonymous');
    END IF;
END
$research$;

DELETE FROM research_values
WHERE origin = 'ordinary';
