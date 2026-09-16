-- The password is supplied as a bound session setting by --migrate, never embedded here.
DO $$
BEGIN
    EXECUTE format(
        'CREATE ROLE prediction_bot_app LOGIN PASSWORD %L',
        current_setting('prediction_bot.runtime_password')
    );
END
$$;
GRANT prediction_bot_runtime TO prediction_bot_app;
