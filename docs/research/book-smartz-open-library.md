# Book Smartz: Open Library search and work metadata

Research for [Investigate Open Library search and work metadata](https://github.com/LowkeyLab/bazel-repo/issues/2216),
part of [Plan the Book Smartz bisection-ranking MVP](https://github.com/LowkeyLab/bazel-repo/issues/2214).
Reviewed on 2026-10-14. This document supplies facts and recommendations; it does
not approve a metadata schema or implement the catalog adapter.

## Existing identity boundary

At repository commit `c158ad6f5dd1611bc328eb27784c382078fa236b`,
[the approved domain design](../superpowers/specs/2026-09-27-book-ranking-domain-design.md)
and [identity implementation](../../book_smartz/domain/identity.rs) associate one
local UUID `BookId` with one `OpenLibraryWorkId`. Work IDs contain `OL`, positive
ASCII decimal digits without a leading zero, and `W`. Identical registration is
idempotent; conflicting mappings are rejected. Editions and translations of the
same work share ranking identity. Metadata and external catalog reads do not
participate in ranking replay.

[The storage design](../superpowers/specs/2026-10-01-book-smartz-postgres-design.md)
and [book registration](../../book_smartz/storage/books.rs) preserve this mapping
in PostgreSQL. Selecting a search result must look up its work before allocating
a new local identity. Do not substitute an edition ID, ISBN, title/author hash,
or a newly generated UUID for an already registered work.

## Documented search contract

Use `GET https://openlibrary.org/search.json`. Search supports free-text `q`,
explicit title and author parameters, requested `fields`, relevance ordering,
and either `offset`/`limit` or one-based `page`/`limit`. Request an explicit field
list; `fields=*` is expensive. Results default to works, with some edition-derived
information. The schema is expressly unstable. Language preference influences
results but does not filter them; nested `editions` can expose a matching edition
without changing the parent work identity. The docs describe only one nested
edition, not edition pagination. [Search API](https://openlibrary.org/dev/docs/api/search).

For initial search, request
`key,title,author_name,author_key,cover_i,first_publish_year,edition_count`.
This is a proposed adapter field list, not an existing application constant.
It supplies display fields and disambiguation hints without requesting every
work, edition, or author separately. Work metadata and edition-specific jackets
are different concepts; do not describe a representative cover as the exact
edition a reader owns. The modern search API is preferred over the legacy generic
book API. [Book APIs](https://openlibrary.org/dev/docs/api/books).

For explicit work lookup, `/works/{work_id}.json` is documented.
Authors have their own `.json` endpoint. Full author records cannot be batched;
author search supports batches of limited metadata. Search-result author names
therefore avoid that fanout. Missing names must have a display fallback, rather
than forcing author resolution for every result.
[Book APIs](https://openlibrary.org/dev/docs/api/books),
[Authors API](https://openlibrary.org/dev/docs/api/authors).

## Catalog ambiguity and identity normalization

Duplicates, author conflation, and incorrectly associated editions are recognized
catalog problems. Librarians can merge duplicate work records and move editions
between works. The catalog is editable; matching titles and authors do not prove
identical works. [Editing FAQ](https://openlibrary.org/help/faq/editing),
[Getting started guide](https://openlibrary.org/help/faq/editing/getting-started-guide).

Recommendation: accept either a bare work key or exactly `/works/{work_id}` at the
adapter boundary, remove only the recognized prefix, then call the existing
`OpenLibraryWorkId` validator. Reject edition keys, arbitrary URLs, slugs, empty
IDs, and invalid syntax. Construct external links from the validated ID.
Deduplicate repeated identical work keys; preserve distinct work keys and show
metadata that helps the reader choose. Never merge by title and author.

A future redirect or upstream merge must not silently rewrite a registered
mapping or coalesce ranking entries. Preserve the old local identity and cached
display data, surface unavailable/redirected metadata, and defer reconciliation
to an explicitly approved policy. This follows the existing domain design's
merge boundary, rather than introducing a new ranking rule.

## Covers and reuse requirements

Use `https://covers.openlibrary.org/b/id/{cover_i}-M.jpg?default=false` for a
positive cover ID. The documented sizes are S, M, and L. A missing cover normally
returns a blank image; `default=false` requests a 404. Public-facing pages should
link images directly to `covers.openlibrary.org`. A courtesy backlink is
appreciated. Cover access by identifiers other than CoverID and OLID has a
separate documented limit of 100 requests per IP per five minutes, with 403 on
excess. Covers must not be crawled in bulk.
[Covers API](https://openlibrary.org/dev/docs/api/covers).

Recommendation: omit the image request when `cover_i` is absent, nonpositive, or
invalid. Use a local placeholder on image errors, including 404, blocking, and
network failures. Cover absence must not prevent registration or comparison.
Use a generated URL, rather than trusting an upstream or browser-supplied URL.
A cover ID avoids the documented ISBN/OCLC/LCCN limit; it does not imply unlimited
service capacity. The choice to embed external images, including its browser
network implications, belongs to the metadata/UI decision.

Open Library asks for useful human-triggered requests, response caching,
application identification with a User-Agent containing application name and
contact email/phone, and application registration. The stated limits are 1 request
per second unidentified and 3 identified. Do not spread load across IPs, perform
hundreds of individual lookups, or treat the API as high-traffic infrastructure.
Bulk needs should use dumps or contact Open Library. These are current policies,
not an availability promise. [API usage guidelines](https://openlibrary.org/developers/api).

Internet Archive asserts no new proprietary rights over database material, but
notes that existing rights may apply to contributions. That statement does not
establish a blanket public-domain license for cover artwork. Link back to the
source and retain provenance; caching metadata and copying cover art are separate
decisions. [Licensing](https://openlibrary.org/developers/licensing).

## Sampled responses, not guarantees

Five public GETs were sampled sequentially on 2026-10-14, with
`User-Agent: BookSmartzResearch/0.1 (https://github.com/LowkeyLab/bazel-repo)`.
That identifies this research and its public project contact; it does not supply
the production email/phone configuration required for the higher limit. No load,
rate-limit, outage, or exhaustive catalog testing was performed.

| Request                                                                                                                                                                                                                        | Observed result                                                                                                                                                                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Title + author, page 1, five results](https://openlibrary.org/search.json?title=the%20lord%20of%20the%20rings&author=tolkien&fields=key,title,author_name,author_key,cover_i,first_publish_year,edition_count&limit=5&page=1) | HTTP 200; both `numFound` and `num_found` were 39, `numFoundExact=true`, `start=0`. First key `/works/OL27448W`, title The Lord of the Rings, author J.R.R. Tolkien, cover 14625765, 253 editions. Results included collections and related works. |
| [Same search, page 2](https://openlibrary.org/search.json?title=the%20lord%20of%20the%20rings&author=tolkien&fields=key,title,author_name,author_key,cover_i,first_publish_year,edition_count&limit=5&page=2)                  | HTTP 200; `start=5`. `/works/OL34714467W` had the same displayed title/author, one edition, and no `cover_i`. This demonstrates ambiguity, not a verified duplicate. Two further rows also lacked `cover_i`.                                       |
| [General author text](https://openlibrary.org/search.json?q=tolkien&fields=key,title,author_name,author_key,cover_i,first_publish_year,edition_count&limit=2&page=1)                                                           | HTTP 200; total 2071, with The Hobbit and The Fellowship of the Ring first. Author text can discover books in one search.                                                                                                                          |
| [Work JSON](https://openlibrary.org/works/OL27448W.json)                                                                                                                                                                       | HTTP 200; author was a reference to `/authors/OL26320A`, not an inline name. The first of nine cover IDs matched the search cover; this sample does not establish a permanent selection rule.                                                      |
| [Invalid cover ID with default disabled](https://covers.openlibrary.org/b/id/0-M.jpg?default=false)                                                                                                                            | HTTP 404, empty body. The adapter should reject this ID locally; the request verified missing-image behavior only.                                                                                                                                 |

Search examples in the documentation include both bare and prefixed work keys
and different total-count spellings. Normalize defensively rather than binding
application behavior to one sampled encoding.
[Search API examples](https://openlibrary.org/dev/docs/api/search).

Raw response bodies and headers were retained outside the checkout in the private
runtime directory
`/home/tacascer/.local/state/worktree-pool/agent-receipts/book-smartz-open-library.samples-af1dc009`.
The table captures the observations for readers without access to that directory;
linked requests are reproducible but their future contents can change.

## Proposed bounded server-side adapter

These names and bounds are recommendations for the next decision ticket, not
implemented APIs or provider guarantees.

| Boundary            | Recommendation                                                                                                                                                                                                                                                       |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Search input        | A trimmed UTF-8 text value with an application maximum, explicit free-text/title/author mode, one-based bounded page, and a fixed page size such as 20. Encode query parameters; do not expose raw Solr syntax by accident.                                          |
| Search output       | Normalized work ID, title, author-name list, optional positive cover ID, optional first-publication year and edition count, page metadata, and optional total/exactness. No match is a successful empty result.                                                      |
| Response validation | Bound response bytes, rows, text lengths, and author-list lengths. Ignore unknown fields. Missing optional fields have fallbacks. A missing/invalid work key cannot become a selectable result; malformed required envelope data is an upstream-response error.      |
| Network ownership   | One server adapter with deadlines, cancellation, aggregate request budgeting across replicas, caching and in-flight request coalescing. Start conservatively within the default limit until real contact identification is configured.                               |
| Errors              | Distinguish invalid local input, local budget exhaustion, upstream limiting/blocking (429/403), timeout, transport/unavailable (5xx), invalid response, and absent work (404). Preserve valid Retry-After information when present; do not assume it is always sent. |
| Selection           | Obtain metadata from a server-held result/cache or bounded validation lookup; do not trust browser-supplied display fields. Resolve the local BookId/work mapping once and preserve registration idempotency.                                                        |
| Saved rankings      | Serve locally retained metadata or a fallback alongside the authoritative ranking. Catalog/cover outages must not prevent ranking load, replay, resume, or answering an existing pair.                                                                               |

The reviewed docs provide no contractual uptime, response-time, completeness,
consistent pagination, or immutable-result guarantee. Do not infer these from
successful GETs. Adapter deadlines, cache expiry, stale-data behavior, retries,
and overload responses are application policy. Offset pages can change while the
catalog or relevance changes; retain IDs as identities, not row positions.

## Remaining decisions and acceptance seams

[Choose book discovery and display-metadata ownership](https://github.com/LowkeyLab/bazel-repo/issues/2218)
should settle search mode and limits, metadata persistence and refresh ownership,
missing-title/author copy, cache freshness and stale fallback, language/edition
display, external cover embedding, and explicit handling of upstream merges.
Production identification/contact and optional application registration remain
setup details once the integration is chosen.

Meaningful acceptance cases include prefixed and bare keys; edition rejection;
repeated work keys versus distinct matching titles; missing author/cover fields;
empty and malformed responses; page offsets; 403/429/5xx and timeouts; oversized
responses; local budget exhaustion; cache reuse; existing registration reuse; and
loading/continuing a ranking while catalog and covers are unavailable. Use an
HTTP test server and controlled responses for these cases, with an optional
manual live smoke test. No production availability claim follows from this
research's successful samples.
