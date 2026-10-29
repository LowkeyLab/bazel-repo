# Book Smartz copy

This schedule updates the copy from [#2720](https://github.com/LowkeyLab/bazel-repo/issues/2720).
Use the approved wording exactly. See the [glossary](GLOSSARY.md) for Ranking, Comparison, Choice, Original action, and Action evidence.
The [acceptance record](COPY-ACCEPTANCE.md) records the earlier copy audit and the packaged checks.

Show the reader what to do next when an action is necessary.
Remove routine explanations of ranking mechanics, expiry periods, refresh behavior, and save processing.
Keep the comparison question, controls, confirmed results, and short instructions for blocked or uncertain saves.
Keep technical failure reasons in private diagnostics.
Use “Your books” for the collection heading, navigation, links, and page title.
Omit subtitles that repeat the purpose of the collection.

<!-- Product messages retain their exact approved first-person wording and submission ellipsis. -->
<!-- Approved product copy uses check for inspection. -->
<!-- vale Google.WordList = NO -->
<!-- vale Google.We = NO -->
<!-- vale Google.Ellipses = NO -->

### Home page

| Element            | Final copy                                                                                                       |
| ------------------ | ---------------------------------------------------------------------------------------------------------------- |
| Brand              | Book Smartz                                                                                                      |
| Headline           | Books, ranked, not rated.                                                                                        |
| Explanation        | Put the books you read in order of preference. Choose between two books at a time to build your private ranking. |
| Primary action     | Sign in with Google                                                                                              |
| Action explanation | Find your first book.                                                                                            |
| Page title         | Book Smartz · Books, ranked, not rated                                                                           |
| Meta description   | Rank the books you read. Choose between two books at a time to build your private ranking with Book Smartz.      |

The first book takes position 1 without a comparison.
The explanation describes how readers build the ranking as they add more books.
The action explanation makes the first step clear.

### Navigation and ranking

| Element                     | Final copy               |
| --------------------------- | ------------------------ |
| Search navigation           | Find a book              |
| Ranking navigation          | Your books               |
| Navigation accessible label | Books                    |
| Brand icon accessible label | Book Smartz              |
| Workspace accessible label  | Books workspace          |
| Workspace heading           | Find a book              |
| Search field label          | Title or author          |
| Search action               | Search books             |
| Ranking heading             | Your books               |
| Empty ranking               | You have no books yet.   |
| Empty ranking next step     | Find a book you read.    |
| Sign-out action             | Sign out of this browser |
| Ranking page title          | Your books · Book Smartz |

Keep the empty-state explanation when the ranking contains no completed entries.
An unfinished first comparison must remain visible separately.

### Search and book selection

| Element or condition                                       | Final copy                                                                                    |
| ---------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Search page heading                                        | Find a book                                                                                   |
| Search page title                                          | Find a book · Book Smartz                                                                     |
| Search results accessible label                            | Search results                                                                                |
| Results return link                                        | Your books                                                                                    |
| Search action on results page                              | Search books                                                                                  |
| Previous page action                                       | Previous page                                                                                 |
| Next page action                                           | Next page                                                                                     |
| Book selection action                                      | Select this book                                                                              |
| Selected book heading                                      | Selected book                                                                                 |
| Selected book page title                                   | Selected book · Book Smartz                                                                   |
| Add action                                                 | Add to your books                                                                             |
| Return link                                                | Return to your books                                                                          |
| No matching books                                          | We found no matching books. Try another title or author.                                      |
| Results contain no books that readers can select           | We found no books you can select on this page. Try another title or author.                   |
| Search rate limit                                          | Wait a moment, then search again.                                                             |
| Search unavailable                                         | Search is unavailable. Try again.                                                             |
| Search timeout                                             | Search is unavailable. Try again.                                                             |
| Invalid search response                                    | Search is unavailable. Try again.                                                             |
| Search response exceeds the size limit                     | Search is unavailable. Try again.                                                             |
| Invalid query or page                                      | Enter a title or author with 1 to 256 characters. Return to your books to start a new search. |
| Add blocked by unfinished comparisons or an uncertain save | Finish your comparisons or check your earlier save before you add another book.               |

Show no expiry notice on the selection page.
If selection or Add fails authorization, instruct the reader to search for the book again.
Keep failure reasons distinct in private diagnostics.
The existing results form keeps its current page number when the reader changes the query.
The copy does not promise that this action returns to page 1.

### Comparisons

| Element or condition        | Final copy                                                           |
| --------------------------- | -------------------------------------------------------------------- |
| Comparison section heading  | Compare books                                                        |
| Comparison accessible label | Book comparisons                                                     |
| New book label              | New book                                                             |
| Saved book label            | Your book                                                            |
| New book summary            | New book: {title}                                                    |
| Question                    | Which book do you prefer?                                            |
| Choice group legend         | Choose the book you prefer                                           |
| Choice action               | Prefer {title}                                                       |
| Pause action                | Pause for now                                                        |
| Resume action               | Resume comparisons                                                   |
| Paused state                | You paused the comparisons for this book. Resume when you are ready. |
| Missing title               | Title unavailable                                                    |

Do not display routine saved-choice counts.
A pause preserves earlier choices, but it does not give the new book a ranking position.

### Saving and confirmed results

A check inspects the saved result. Confirmation establishes that the save succeeded.

| Condition                              | Final copy                                 |
| -------------------------------------- | ------------------------------------------ |
| Submission in progress                 | Saving…                                    |
| Book accepted, comparisons unfinished  | Book added.                                |
| Choice saved, comparisons unfinished   | We saved your choice.                      |
| Pause confirmed                        | We paused the comparisons.                 |
| Resume confirmed                       | We resumed the comparisons.                |
| Ranking position confirmed             | We saved this book at position {position}. |
| Repeat of a saved action               | We already saved this.                     |
| Original save confirmed after recovery | We confirmed your save.                    |

Keep “Saving…” until a server response confirms the result.
Only a completed ranking entry can receive a saved position message.
A repeated action must not appear to create a second saved result.
Recovery confirmation does not imply that an unfinished book received a position.

### Uncertain saves and recovery

| Element or condition                                 | Final copy                                                                |
| ---------------------------------------------------- | ------------------------------------------------------------------------- |
| Recovery heading and action                          | Check your save                                                           |
| Unresolved save blocks changes                       | We cannot confirm your save yet. Check it before you change your books.   |
| Uncertain result after submission                    | We cannot confirm your save yet. Check it before you try again.           |
| An earlier save needs checking before another Add    | Check your save before you add another book.                              |
| An earlier save needs checking before another choice | Check your save before you choose another book.                           |
| Recovery page title                                  | Check your save · Book Smartz                                             |
| Recovery return link                                 | Return to your books                                                      |
| Recovery offers a return link without a check form   | We cannot confirm your save yet. Return to your books to check your save. |

Use the direct check instruction only when the response includes the recovery form.
Use the return instruction when the response offers only the ranking link.
The action refers to the original submitted action.
It must not create a new choice, Add, Pause, or Resume.

### Rejected or changed actions

| Condition                                | Final copy                                                                 |
| ---------------------------------------- | -------------------------------------------------------------------------- |
| Rejection page title                     | Review your books · Book Smartz                                            |
| Rejection heading                        | Review your books                                                          |
| Book already ranked                      | You already added this book.                                               |
| Unfinished comparisons block another Add | Finish the comparisons for your current book before you add another book.  |
| Ranking changed since submission         | Your books changed. Review them before you try again.                      |
| Comparison changed since submission      | Your comparison changed. Review the current books before you choose again. |

These messages request a fresh decision after the reader reviews the current state.
They must not suggest that an uncertain action failed.
Apply new rejection wording to future actions.
Keep historical action evidence intact.

### Access errors

| Condition                             | Final copy                                                 |
| ------------------------------------- | ---------------------------------------------------------- |
| Sign-in required                      | Sign in to view your books.                                |
| Selection or Add authorization failed | Search for this book again.                                |
| Other action authorization failed     | Return to your books and review them before you try again. |

Keep the existing sign-in or ranking link with each instruction.

### Book metadata and accessibility

| Element                        | Final copy                            |
| ------------------------------ | ------------------------------------- |
| Missing author                 | Author unavailable                    |
| Missing cover alternative text | Cover unavailable                     |
| Cover accessible label         | Example cover for this book           |
| Brand                          | Book Smartz                           |
| Font licence titles            | Keep the existing font licence names. |

Keep catalog titles, author names, publication years, and source links unchanged.
An example cover does not identify a specific edition.
Keep accessible names consistent with visible actions.

<!-- vale Google.Ellipses = YES -->
<!-- vale Google.We = YES -->
<!-- vale Google.WordList = YES -->
