# Book Smartz personal-library prototype

This throwaway prototype compares three layouts within the approved visual direction.
It answers the layout question in [Prototype the Book Smartz personal library](https://github.com/LowkeyLab/bazel-repo/issues/2615).
The maintainer must review the layouts before the decision closes.

Run the preview from this branch:

```bash
nix develop --command bazel run //book_smartz/web:prototype --remote_cache= --remote_executor= --bes_backend=
```

Open <http://127.0.0.1:2615/ranking?variant=A>.
Use the bottom arrows to select a layout. The URL retains the selection.
Use the Screen control to inspect each journey and recovery state.
Use Simulate server confirmation to separate submission feedback from a confirmed fixture response.

| Variant            | Layout question                                                                      |
| ------------------ | ------------------------------------------------------------------------------------ |
| A: Cover shelf     | Does a white task surface with a ranking on the right give each choice enough focus? |
| B: Comparison desk | Does a ranking on the left give the reader useful context beside larger covers?      |
| C: Reading list    | Do compact book rows make long titles and repeated comparisons easier to read?       |

The captured layouts show both screen sizes.

| Layout | Desktop                                                   | Phone                                                    |
| ------ | --------------------------------------------------------- | -------------------------------------------------------- |
| A      | [Cover shelf](prototype-review/A-comparison-1440.png)     | [Cover shelf](prototype-review/A-comparison-390.png)     |
| B      | [Comparison desk](prototype-review/B-comparison-1440.png) | [Comparison desk](prototype-review/B-comparison-390.png) |
| C      | [Reading list](prototype-review/C-comparison-1440.png)    | [Reading list](prototype-review/C-comparison-390.png)    |

All layouts place the active task before saved rankings on phones.
The shared Tailwind theme uses the approved palette, `Literata` headings, and Source Sans 3 controls.
The prototype uses the existing `topcoat_tailwind` wrapper and declared local assets.
Frequent comparisons change immediately. The prototype adds no motion.
The later feedback decision specifies the motion contract.

Export a standalone file with embedded styles, fonts, and covers:

```bash
nix develop --command bazel run //book_smartz/web:prototype --remote_cache= --remote_executor= --bes_backend= -- --export /tmp/book-smartz-prototype.html
```

Run the local browser review:

```bash
nix develop --command bazel run //book_smartz/web:prototype --remote_cache= --remote_executor= --bes_backend= -- --review /tmp/book-smartz-prototype-review
```

The review checks 15 scenes in three variants at widths of 390 and 1440 pixels.
It checks page errors, missing images, horizontal overflow, submission confirmation, Pause/Resume, and keyboard variant selection.
It captures screenshots of comparison, search, and original-action recovery screens.
The screenshots support visual review. Complete accessibility and production acceptance need separate checks.

## Limits

The prototype uses fixed book data and simulated server responses.
It has no account, database, persistence, provider calls, or production mutations.
Authentication, security checks, ranking rules, native form transport, and recovery correctness need separate production checks.
Those behaviors remain requirements for production implementation.
No production package depends on the preview target. Keep this branch outside main.

## Asset sources

The local font files came from the Google Fonts repository on 2026-10-23.
The adjacent font license files contain their license terms.

- [Heading font](https://github.com/google/fonts/tree/main/ofl/literata)
- [Source Sans 3](https://github.com/google/fonts/tree/main/ofl/sourcesans3)
- [Dune cover](https://covers.openlibrary.org/b/isbn/9780441172719-M.jpg)
- [The Left Hand of Darkness cover](https://covers.openlibrary.org/b/isbn/9780441478125-M.jpg)

Piranesi and the third ranked book deliberately use missing-cover fixtures.
The committed assets preserve the exact prototype inputs.
