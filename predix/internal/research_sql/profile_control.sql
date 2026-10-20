CREATE FUNCTION research_branch(taken BOOLEAN)
RETURNS INTEGER
LANGUAGE plpgsql
AS $profile$
BEGIN
    IF taken THEN
        RETURN 23;
    ELSE
        RETURN 97;
    END IF;
END
$profile$;
