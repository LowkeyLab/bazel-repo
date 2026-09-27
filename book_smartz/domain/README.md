# Book Smartz ranking domain

`book_smartz_domain` is a pure, in-memory library for one reader's ordered book
ranking. A caller registers books by its own UUID and an Open Library **work** ID,
then supplies a reader UUID, a fresh event UUID, an expected revision, and a UTC
instant for each command. The library does not read a clock, generate IDs, fetch
catalog data, or authenticate readers.

```rust
use book_smartz_domain::{
    Book, BookId, BookRegistry, Command, CommandContext, ComparisonChoice, EventId,
    OpenLibraryWorkId, Ranking, ReaderId, decode_event, encode_event,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

fn example() -> Result<(), Box<dyn std::error::Error>> {
let reader = ReaderId::new(Uuid::from_u128(100));
let a = BookId::new(Uuid::from_u128(1));
let b = BookId::new(Uuid::from_u128(2));
let mut books = BookRegistry::new();
books.register(Book::new(a, OpenLibraryWorkId::try_from("OL1W")?))?;
books.register(Book::new(b, OpenLibraryWorkId::try_from("OL2W")?))?;

let mut ranking = Ranking::new(reader);
let at = |second| {
    DateTime::parse_from_rfc3339(&format!("2026-09-27T12:00:{second:02}Z"))
        .unwrap()
        .with_timezone(&Utc)
};
let mut execute = |sequence, command| {
    ranking.execute(
        &books,
        CommandContext {
            expected_revision: sequence - 1,
            event_id: EventId::new(Uuid::from_u128(sequence.into())),
            time: at(sequence),
        },
        command,
    )
};
execute(1, Command::Start { book_id: a })?; // first book ranks immediately
execute(2, Command::Start { book_id: b })?;
execute(3, Command::Answer {
    opponent: a,
    choice: ComparisonChoice::PreferCandidate,
})?; // ranking is now B > A

let decoded = ranking.history().iter().map(|event| {
    let json = encode_event(event)?;
    Ok(decode_event(&json)?.event().clone())
}).collect::<Result<Vec<_>, book_smartz_domain::CodecError>>()?;
let rebuilt = Ranking::from_history(reader, decoded)?;
assert_eq!(rebuilt.projection(), ranking.projection());
Ok(())
}
```

The code above illustrates the calls; an application supplies its own UUIDs and
instants. Registering the same book/work
mapping again is idempotent. A different book UUID for the same work ID, or a
different work ID for the same book UUID, is rejected.

Only one placement can be active per reader. Read
`ranking.projection().next_opponent()` to get the selected pair and answer it with
`PreferCandidate`, `PreferOpponent`, or `Skip`. A skip keeps the insertion bounds
but excludes that opponent for the current round. When all useful opponents have
been skipped, placement pauses automatically. `Command::Pause` pauses an active
placement explicitly; `Command::Resume` starts a new round, clearing skips while
keeping prior winning decisions, insertion bounds, and `started_at`. A pending
book is absent from ranked entries until a decision completes its placement.

`ranking.history()` is the authoritative ordered stream. Each accepted command
adds exactly one event; rejected duplicate, stale, or invalid commands add none.
`Ranking::from_history` validates sequence, reader, event IDs, and placement
transitions while deriving the current order and original timestamps. Retain the
history if the ranking must survive beyond this process: this package has no
storage or restart durability. Replaying history does not publish telemetry or
turn old activity into new activity.

`encode_event` and `decode_event` support the library's CloudEvents 1.0 JSON
profile. Events use `specversion: "1.0"`, `source: "urn:uuid:<reader-id>"`,
`subject: "books/<candidate-book-id>"`, a required UTC `time`, and JSON `data`.
The data contains `readerId`, `candidateBookId`, and a positive decimal-string
`sequence`; comparison data also contains `opponentBookId` and `choice`.
The supported authoritative types are `bookranking.placement.started.v1`,
`bookranking.comparison.answered.v1`, `bookranking.placement.paused.v1`, and
`bookranking.placement.resumed.v1`. The whole document has media type
`application/cloudevents+json`; its `datacontenttype` is `application/json`.
The codec is pure and does not provide a transport or durable event store.

Run the package tests with
`nix develop --command aspect test //book_smartz/domain:domain_test`.
