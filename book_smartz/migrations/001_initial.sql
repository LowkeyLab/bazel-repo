CREATE TABLE book_smartz.books (
    id UUID PRIMARY KEY,
    work_id TEXT NOT NULL UNIQUE CHECK (work_id ~ '^OL[1-9][0-9]*W$')
);

CREATE TABLE book_smartz.reader_streams (
    reader_id UUID PRIMARY KEY
);

CREATE TABLE book_smartz.ranking_events (
    reader_id UUID NOT NULL REFERENCES book_smartz.reader_streams(reader_id),
    sequence CHAR(20) COLLATE "C" NOT NULL,
    event_id UUID NOT NULL,
    candidate_book_id UUID NOT NULL REFERENCES book_smartz.books(id),
    opponent_book_id UUID REFERENCES book_smartz.books(id),
    envelope JSONB NOT NULL,
    PRIMARY KEY (reader_id, sequence),
    UNIQUE (reader_id, event_id),
    CONSTRAINT valid_sequence CHECK (
        length(sequence::text) = 20
        AND sequence::text ~ '^[0-9]{20}$'
        AND sequence > '00000000000000000000'
        AND sequence <= '18446744073709551615'
    ),
    CONSTRAINT valid_envelope CHECK (
        (jsonb_typeof(envelope) = 'object') IS TRUE
        AND (envelope->>'specversion' = '1.0') IS TRUE
        AND (envelope->>'id' = event_id::text) IS TRUE
        AND (envelope->>'source' = 'urn:uuid:' || reader_id::text) IS TRUE
        AND (envelope->>'subject' = 'books/' || candidate_book_id::text) IS TRUE
        AND (envelope->>'time' <> '') IS TRUE
        AND (envelope->>'datacontenttype' = 'application/json') IS TRUE
        AND (envelope->>'sequence' = sequence::text) IS TRUE
        AND (jsonb_typeof(envelope->'data') = 'object') IS TRUE
        AND (envelope->'data'->>'readerId' = reader_id::text) IS TRUE
        AND (envelope->'data'->>'candidateBookId' = candidate_book_id::text) IS TRUE
        AND (envelope->'data'->>'sequence' = (sequence::text)::numeric::text) IS TRUE
        AND (
            (
                (envelope->>'type' = 'bookranking.comparison.answered.v1') IS TRUE
                AND opponent_book_id IS NOT NULL
                AND (envelope->'data'->>'opponentBookId' = opponent_book_id::text) IS TRUE
                AND (envelope->'data'->>'choice' IN ('skip', 'prefer_candidate', 'prefer_opponent')) IS TRUE
            ) OR (
                (envelope->>'type' IN (
                    'bookranking.placement.started.v1',
                    'bookranking.placement.paused.v1',
                    'bookranking.placement.resumed.v1'
                )) IS TRUE
                AND opponent_book_id IS NULL
                AND NOT (envelope->'data' ? 'opponentBookId')
            )
        )
    )
);
