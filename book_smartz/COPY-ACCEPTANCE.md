# Book Smartz copy acceptance

## Current copy

The [current schedule](COPY.md) removes routine ranking, expiry, saved-choice, and recovery explanations.
Save confirmations and error messages use short instructions without internal action terminology.
The collection heading, navigation, and links use “Your books”.
The collection omits the subtitle about preference order.
Access errors retain a sign-in or collection link.
Selection and Add authorization failures instruct the reader to search for the book again.
The HTTP, app, and browser checks cover these changes through the packaged app.
They retain checks for saved state, comparison identity, Pause, Resume, and uncertain saves.

## Baseline copy audit

The combined app at [`9bec81d7e69cf08c8fd0ffbdaa4cf2d911c70bef`](https://github.com/LowkeyLab/bazel-repo/commit/9bec81d7e69cf08c8fd0ffbdaa4cf2d911c70bef) implemented all 87 rows of the earlier schedule.
This baseline is a source audit. The completion record in [#2725](https://github.com/LowkeyLab/bazel-repo/issues/2725) supplies the exact tested commit, invocation links, durations, and execution results.

Eighty-three rows have literal production text. Three rows expand the approved title or position templates from confirmed state.
The remaining row retains the existing font licence titles, `Literata font license` and `Source Sans 3 font license`.
The book summary combines its approved prefix with the catalog title. Both choice buttons name their respective books.
Only a fresh completed receipt supplies a position. Replay and recovery retain their distinct messages.

| Schedule                                                                   | Rows | Production location                                                                                    | Existing acceptance boundary                                                                               |
| -------------------------------------------------------------------------- | ---- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------- |
| [Home page](COPY.md#home-page)                                             | 7    | [main.rs](web/main.rs)                                                                                 | Chromium sign-in entrypoint and approved headline, explanation, and sign-in action                         |
| [Navigation and ranking](COPY.md#navigation-and-ranking)                   | 15   | [main.rs](web/main.rs), [views.rs](web/views.rs), [catalog.rs](web/catalog.rs)                         | Chromium empty and populated rankings, accessible navigation, and browser sign-out                         |
| [Search and book selection](COPY.md#search-and-book-selection)             | 22   | [catalog.rs](web/catalog.rs)                                                                           | Packaged HTTP catalog scenarios and Chromium search, selection, pagination, expiry, and provider failures  |
| [Comparisons](COPY.md#comparisons)                                         | 15   | [actions.rs](web/actions.rs)                                                                           | Packaged app zero, one, and multiple choices, Pause/Resume, restart, and pending-versus-ranked state       |
| [Saving and confirmed results](COPY.md#saving-and-confirmed-results)       | 8    | [feedback.rs](web/feedback.rs), [submitting.js](web/submitting.js)                                     | Held Chromium responses and packaged feedback scenarios across Add, choice, Pause, and Resume              |
| [Uncertain saves and recovery](COPY.md#uncertain-saves-and-recovery)       | 9    | [actions.rs](web/actions.rs)                                                                           | Packaged failure and restart scenarios, return-only guidance, native Chromium recovery, and response focus |
| [Rejected or changed actions](COPY.md#rejected-or-changed-actions)         | 6    | [actions.rs](web/actions.rs)                                                                           | Future rejection facts, exact historical reasons, stale comparison feedback, and preserved histories       |
| [Book metadata and accessibility](COPY.md#book-metadata-and-accessibility) | 5    | [actions.rs](web/actions.rs), [views.rs](web/views.rs), [catalog/metadata.rs](web/catalog/metadata.rs) | Chromium long titles, missing metadata, controlled cover failure, accessible names, and local font assets  |

Historical rejection facts retain their recorded reasons. New rejection facts use the approved wording.
Existing contracts cover action identity, signed forms, expected revisions, reader authorization, Origin validation, and cross-site request forgery protection.
The glossary records Ranking, Comparison, and Choice. Original action and Action evidence retain their exact definitions.

## Fresh packaged execution

Run the existing packaged checks with the tested commit in their evidence:

```bash
nix develop --command aspect test \
  //book_smartz/web:http_test \
  //book_smartz/web:application_test \
  //book_smartz/web:browser_test \
  //book_smartz/web:package_test \
  --nocache_test_results \
  --noremote_accept_cached \
  --flaky_test_attempts=1 \
  --test_env=BOOK_SMARTZ_ACCEPTANCE_COMMIT="$(git rev-parse HEAD)"
```

The existing HTTP and app scenarios use the delivered package, real PostgreSQL, production migrations, and concrete provider clients.
Controlled Google and Open Library endpoints supply provider responses.
The browser journey covers sign-in, search, selection, Add, comparisons, Pause, Resume, rejection, replay, and recovery.
Add includes the first book and later books.
It also checks keyboard operation, response focus, accessible names, status announcements, and native forms without JavaScript.
Held responses distinguish submission from confirmation. State assertions retain original identities, frozen effects, saved choices, and unfinished books.

The browser records scenes at 320, 390, and 1440 pixels. Existing assertions verify visible control bounds and computed contrast.
Inspect the longer-copy, uncertainty, rejection, and native comparison screenshots.
`browser-evidence.json` and `motion-evidence.json` identify the tested commit and Chromium version.
Motion checks cover reduced motion and the rules for a fresh confirmed position highlight.
Package checks verify delivered asset integrity, local fonts, runtime files, and launch from unrelated directories.

Run the repository-required generation, full formatting, full test suite, full build, and lint sequence.
Record source reviews and hosted CI against the final PR head before merge.

## Limits

Controlled-provider results do not establish live Google or Open Library acceptance.
The browser aborts cover requests. These checks do not establish successful live cover delivery or deployment acceptance.
Fixture ownership describes intended isolation. These checks do not establish parallel isolation.
Computed contrast and control bounds cover the recorded scenes. They do not establish complete accessibility conformance.
