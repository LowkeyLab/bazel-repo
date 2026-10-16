# CloudEvents metadata for Book Smartz action recovery

Research date: 2026-10-16. Resolves the investigation in
[Investigate standard CloudEvents attributes and extensions for action recovery](https://github.com/LowkeyLab/bazel-repo/issues/2290).
This is research evidence, not an implementation or a newly approved contract.

## Finding

Use core event identity and the published Correlation extension before inventing
`operationid` or `rankingeventid`. The extension already defines `correlationid`
for a logical workflow and `causationid` for its direct causing event. The maintainer
[accepted its adoption with a pinned definition](https://github.com/LowkeyLab/bazel-repo/issues/2219#issuecomment-6213189142).
It is published in the official repository, but is neither a normative core
attribute nor part of the v1.0.2 release.
[Correlation specification][correlation], [released catalog][released-catalog].

The accepted business boundaries remain those in
<!-- vale Google.WordList = NO -->

[Choose the Topcoat application and command contract](https://github.com/LowkeyLab/bazel-repo/issues/2219#issuecomment-6213120802):
<!-- vale Google.WordList = YES -->

durable submitted intent before effects; independent book registration; ranking
change and accepted ranking event in one commit; recovery derived by replay.
An envelope does not implement these transactions or guarantee idempotency.

## Source versions and maturity

The official repository identifies **v1.0.2** as the latest CloudEvents release;
its serialized `specversion` remains `1.0`. The current repository revision
inspected was `472f0e1d856e335c30fa204f36fd6428c2bfd732`. Its README describes
Documented Extensions as **not versioned**. The extension catalog explicitly
allows experimental attributes, assigns them no official standing, and permits
change or removal. This applies even to extensions present in a release tree.
[Repository status][upstream], [core][core], [extension policy][catalog].

Correlation exists at the pinned current revision and was added in
[commit a43f836][correlation-added]. It is absent from both the v1.0.2 extension
tree and its documented catalog. The current catalog's Known Extensions list
also omits Correlation, despite the file existing. Describe it as a published
experimental extension, not as a proposal awaiting merge, a vendor convention, or a
ratified standard. Recorded Time and Auth Context are also in the current tree
but absent from v1.0.2. [Current catalog][catalog], [released catalog][released-catalog],
[released extension tree][released-tree].

## Suitability matrix

Every extension below is optional outside an app profile. Requirements
inside an extension apply when that extension is used.

| Requirement                                                | Existing field or extension                                                                              | Semantics and status                                                                                                                         | Suitability and gap                                                                                                                                                                                                                                                                                                                               |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identify an immutable occurrence; recognize retransmission | Core `source` + `id`, v1.0.2                                                                             | Together identify one distinct event; retransmission may retain them.                                                                        | Keep a distinct event identity for submitted intent, prepared command, accepted ranking decision, and rejection. A retry does not create another occurrence. This does not define command deduplication or a database receipt. [Core identity][core-id]                                                                                           |
| Group all facts about one submitted action                 | `correlationid`, current published Correlation extension; absent from v1.0.2                             | Nonempty string grouping events in one logical flow or business transaction.                                                                 | Fits the stable action identity across intent, registration progress, ranking outcome, and recovery. It does not mean those facts share one database transaction. Reuse the original value on retries. [Correlation][correlation]                                                                                                                 |
| Identify the immediate trigger                             | `causationid`, same extension/status                                                                     | Nonempty string identifying the directly causing event; should be that event's `id`.                                                         | Use the actual submitted-intent or command-prepared event that directly causes the outcome. Do not insert an arbitrary HTTP request ID or command ID unless it is also that event's ID. It identifies one cause, not the complete chain. [Correlation][correlation]                                                                               |
| Freeze the command and its expected outcome identity       | Event `data`, app schema                                                                                 | Domain-specific content belongs in data; no inspected extension defines a frozen command or expected ranking revision.                       | Store the full command, authenticated reader, original revision/time, and target ranking event's source and ID in submitted/prepared facts. This is an app data schema, with no custom context extension required. [Primer][primer], [local command][local-command]                                                                               |
| Order one reader's accepted decisions                      | `sequence`, v1.0.2 published extension                                                                   | Nonempty lexically ordered string; v1.0.2 permits agreed semantics without `sequencetype`. Current text explicitly scopes order by `source`. | Keep the existing 20-digit padded positive `u64`, without `sequencetype: Integer` (that mode is wrapping signed 32-bit). A separate app history needs a distinct source if it has its own sequence. Neither version establishes cross-source order or optimistic concurrency. [Released Sequence][sequence-release], [current Sequence][sequence] |
| Distinguish original occurrence from event creation        | Core `time`; published `recordedtime` in current tree                                                    | `time` describes the occurrence. `recordedtime` describes when the producer created that event; intermediaries must preserve it.             | Preserve the accepted original command instant and existing ranking time semantics. Different app facts have their own occurrence times. Add `recordedtime` only when event-creation time is needed; it is not database commit time or retry-attempt time. [Core time][core-time], [Recorded Time][recordedtime]                                  |
| Follow distributed diagnostics                             | `traceparent`, optional `tracestate`; published Distributed Tracing extension in v1.0.2 and current tree | Carries W3C trace context; current text specifies transmission-origin trace context across multiple hops.                                    | Useful optional diagnostics. Trace context has execution/propagation semantics, not durable action identity, command equality, or commit acknowledgement. Recovery must work without it. [Tracing][tracing]                                                                                                                                       |
| Express authenticated actor                                | `authtype`, optional `authid`/`authclaims`; current published Auth Context extension                     | Informational principal context, explicitly does not secure CloudEvents.                                                                     | Omit from minimum profile. Authenticated reader ownership is already explicit; server authorization remains mandatory. Never persist session tokens or credentials as context. [Auth Context][authcontext]                                                                                                                                        |
| Route a broker partition                                   | `partitionkey`, published extension present in v1.0.2                                                    | Routes grouping to a partition; its value may change or disappear across hops.                                                               | No broker requirement here; unsuitable as durable correlation, causation, or ordering proof. [Partitioning][partitioning]                                                                                                                                                                                                                         |

The core `subject` remains the book subject on ranking events; replacing it with
a command identity would break the existing profile. `dataschema` can identify
a payload schema but cannot prove that an action committed. App-specific
event types and payload schemas are expected producer contracts, not proprietary
extensions. [Core subject/schema][core-subject], [existing codec][local-codec].

## Recommended minimum profile for the human decision

The maintainer accepted Correlation adoption; the remaining details below are
research recommendations for the app decision:

1. Keep the existing ranking envelope profile and canonical UUID event IDs.
   Assign separate UUIDs to different facts; use globally unique values for new
   events so a `causationid` is unambiguous even across sources. Retain full
   source-plus-ID references in command data because core identity is a pair.
2. Make the first submitted-intent event's UUID the stable action identity and
   carry it as `correlationid` on facts belonging to that action. This avoids a
   second proprietary operation identifier. Generate it once for the offered
   action; persist the first submitted occurrence without changing it on retry.
3. Freeze the complete command and its intended ranking event identity before
   ranking execution. Add can first record selected-work intent, then resolve the
   canonical registered book before freezing Start. These are facts at different
   stages; do not reuse one event ID with changed contents.
4. Put `causationid` on a resulting fact when its direct cause is a recorded
   intent/prepared event. Omit it on a root intent with no causing event.
   Correlation describes membership; causation describes an edge.
5. Commit the accepted ranking envelope, including correlation context, with the
   ranking change. That event is success evidence. Do not append a separate
   completion event merely to prove success. Definitive rejection is an
   app fact, without advancing ranking sequence.
6. Keep app recovery history's source distinct from the existing ranking
   source when it maintains a separate sequence. Specify its authoritative
   order without pretending that independent sequences form a global order.
   Do not sequence derived notices as additional ranking decisions.
7. Keep trace context optional. Defer recorded time and auth context unless their
   distinct information has a consumer. Preserve any accepted metadata through
   reads, retries, and serialization; replay never executes commands.

These details are inferred from the specifications and accepted boundaries;
the research does not add business decisions. For comparison, if experimental
Correlation metadata had been declined, the smallest fallback is source-plus-ID references and
flow identity **inside app event data**. The research found no need for a
custom top-level extension. A mature-core-only correlation extension does not
exist in the inspected v1.0.2 catalog; pretending otherwise would conceal the
actual maturity trade-off. [Core][core], [catalog][catalog], [Correlation][correlation].

## Scenario checks

| Scenario                                      | Required interpretation                                                                                                                                                                                                  |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Same button submitted twice                   | Same frozen intent/action identity. Compare original contents; a reused identity with changed contents is not an equivalent retry. Core duplicate identity alone does not validate equality.                             |
| Accepted comparison, response lost            | Reconcile the original ranking source/ID and intended contents against authoritative history. The committed accepted event proves success; correlation provides navigation, not proof by itself.                         |
| Commit still in flight; one read sees nothing | Keep uncertainty pending. Neither a missing event nor a missing trace establishes rollback.                                                                                                                              |
| Another tab changes the ranking               | The original expected revision still governs. A stale rejection is a separate app event correlated to the old action, not a new ranking revision or a silent retry against a newer pair.                                 |
| Registration commits, Start rejects           | Catalog registration remains. Correlate this action's progress, but do not claim this action caused a pre-existing shared registration event. Record the observed/resolved mapping as app data when needed.              |
| Signing in again or process restart           | Rebuild pending/outcome status from immutable facts; authenticate and authorize the recovery request independently. Preserve the original command identity, expected revision, contents, and instant on permitted retry. |

These consequences follow the accepted recovery contract and existing storage
recovery documentation, not new guarantees supplied by CloudEvents.
[Accepted boundaries](https://github.com/LowkeyLab/bazel-repo/issues/2219#issuecomment-6213120802),
[storage recovery contract][local-storage-readme].

## Repository constraints

The repository pins `cloudevents-sdk = "=0.9.0"` without default features.
Upstream tag `0.9.0` resolves to
`57196ea45c637fdc71c2372e47910e83dc35c334`. `Event::set_extension` and the
builder accept generic extension values; `ExtensionValue` supports strings,
booleans, and integers. Therefore correlation/causation strings require no new SDK
version. A timestamp extension is represented as its RFC 3339 string in JSON;
the generic SDK does not validate each published extension's semantics.
[Dependency pin][local-cargo], [SDK event][sdk-event], [SDK values][sdk-values],
[SDK builder][sdk-builder].

The Book Smartz codec validates lowercase alphanumeric extension names, allowed
scalar values, signed 32-bit JSON numbers, and context-string characters. It
rejects duplicate JSON keys and reserved-attribute collisions.
`CloudEventDocument` retains the envelope; `encode_document` preserves valid
unknown extensions while normalizing sequence. It explicitly validates trace
presence/type relationships, but not Correlation's nonempty-string semantics or
Recorded Time's timestamp grammar. Those need profile validation if adopted.
Derived notices currently carry `data.causationId`, not the lowercase context
extension, and have no ranking sequence. They remain derived consequences, not
authoritative command outcomes. Building from `RankingEvent` with `encode_event`
recreates the base envelope and
cannot preserve attached context. [Codec][local-codec].

The database stores the envelope as JSONB and checks reader, event, candidate,
and sequence indexes against it. Valid extra attributes are not excluded.
However, the current write path accepts a domain command/context and calls
`encode_event`; it offers no envelope-context input. Its `OperationContext`
correlation UUID is only best-effort diagnostics, not a persisted CloudEvents
attribute. The read path decodes a document then returns only `RankingEvent`,
discarding access to envelope extensions. No durable app-intent/rejection
history exists in this schema. These are storage/interface gaps, not reasons to
invent extension names. [Schema][local-schema], [ranking write/read][local-ranking],
[store interface][local-interface].

Preserve the current domain validation and locked decision/append/commit path;
provide the context and authoritative-envelope access required for recovery.
The existing private transaction ownership must not dictate a different domain
boundary. Future implementation should demonstrate context round trips and
original-event reconciliation through real storage tests. This research changed
no app code and performed no runtime recovery experiment.

## Artifact validation

No existing research-notes directory was present at the inspected repository
revision. This single document establishes `docs/research/` for this throwaway
research branch; it is not a proposal to merge a new documentation convention.
No source files or BUILD files were edited, so source-triggered Gazelle does
not apply. Validation results are recorded in the research ticket's resolution.

[upstream]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/README.md#cloudevents-documents
[core]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md
[core-id]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md#id
[core-time]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md#time
[core-subject]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md#subject
[primer]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/primer.md#cloudevent-attribute-extensions
[catalog]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/README.md
[released-catalog]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/documented-extensions.md
[released-tree]: https://github.com/cloudevents/spec/tree/v1.0.2/cloudevents/extensions
[correlation]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/correlation.md
[correlation-added]: https://github.com/cloudevents/spec/commit/a43f83658ee82e2c6b16382c04c55317c96e5e61
[sequence-release]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/extensions/sequence.md
[sequence]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/sequence.md
[recordedtime]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/recordedtime.md
[tracing]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/distributed-tracing.md
[authcontext]: https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/authcontext.md
[partitioning]: https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/extensions/partitioning.md
[sdk-event]: https://github.com/cloudevents/sdk-rust/blob/57196ea45c637fdc71c2372e47910e83dc35c334/src/event/mod.rs#L216
[sdk-values]: https://github.com/cloudevents/sdk-rust/blob/57196ea45c637fdc71c2372e47910e83dc35c334/src/event/extensions.rs
[sdk-builder]: https://github.com/cloudevents/sdk-rust/blob/57196ea45c637fdc71c2372e47910e83dc35c334/src/event/v10/builder.rs
[local-cargo]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/Cargo.toml#L23
[local-command]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/book_smartz/domain/command.rs
[local-codec]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/book_smartz/domain/cloudevents.rs
[local-schema]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/book_smartz/migrations/001_initial.sql
[local-ranking]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/book_smartz/storage/ranking.rs
[local-interface]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/book_smartz/storage/lib.rs
[local-storage-readme]: https://github.com/LowkeyLab/bazel-repo/blob/e2c10dfb1295d8606657d80eafbb8196831d5f56/book_smartz/storage/README.md
