CREATE TABLE book_smartz.application_events (
    reader_id UUID NOT NULL REFERENCES book_smartz.reader_streams(reader_id),
    sequence CHAR(20) COLLATE "C" NOT NULL,
    event_id UUID NOT NULL,
    envelope JSONB NOT NULL,
    PRIMARY KEY (reader_id, sequence),
    UNIQUE (reader_id, event_id),
    CONSTRAINT valid_application_sequence CHECK (
        length(sequence::text) = 20
        AND sequence::text ~ '^[0-9]{20}$'
        AND sequence > '00000000000000000000'
        AND sequence <= '18446744073709551615'
    ),
    CONSTRAINT valid_application_envelope CHECK (
        (jsonb_typeof(envelope) = 'object') IS TRUE
        AND (envelope->>'specversion' = '1.0') IS TRUE
        AND (envelope->>'id' = event_id::text) IS TRUE
        AND (envelope->>'source' = 'urn:book-smartz:application:' || reader_id::text) IS TRUE
        AND (envelope->>'type' <> '') IS TRUE
        AND (envelope->>'time' <> '') IS TRUE
        AND (envelope->>'datacontenttype' = 'application/json') IS TRUE
        AND (envelope->>'sequence' = sequence::text) IS TRUE
        AND (jsonb_typeof(envelope->'data') = 'object') IS TRUE
    )
);
