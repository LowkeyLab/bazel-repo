# Book Smartz copy

This is the final copy schedule from [#2720](https://github.com/LowkeyLab/bazel-repo/issues/2720).
Use the approved wording exactly. See the [glossary](GLOSSARY.md) for Ranking, Comparison, Choice, Original action, and Action evidence.
The [acceptance record](COPY-ACCEPTANCE.md) maps the schedule to production boundaries and packaged checks.

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
| Action explanation | After sign-in, find your first book to start your ranking.                                                       |
| Page title         | Book Smartz · Books, ranked, not rated                                                                           |
| Meta description   | Rank the books you read. Choose between two books at a time to build your private ranking with Book Smartz.      |

The first book takes position 1 without a comparison.
The explanation describes how readers build the ranking as they add more books.
The action explanation makes the first step clear.

### Navigation and ranking

| Element                     | Final copy                                                                      |
| --------------------------- | ------------------------------------------------------------------------------- |
| Search navigation           | Find a book                                                                     |
| Ranking navigation          | Your ranking                                                                    |
| Navigation accessible label | Book ranking                                                                    |
| Brand icon accessible label | Book Smartz                                                                     |
| Workspace accessible label  | Book ranking workspace                                                          |
| Workspace heading           | Find a book to rank                                                             |
| Search field label          | Title or author                                                                 |
| Search action               | Search books                                                                    |
| Ranking heading             | Your saved ranking                                                              |
| Ranking subheading          | Books in your order of preference                                               |
| Empty ranking               | Your ranking is empty.                                                          |
| Empty ranking next step     | Find a book you read to start your ranking.                                     |
| Unfinished book explanation | Finish the comparisons for each new book to give it a position in your ranking. |
| Sign-out action             | Sign out of this browser                                                        |
| Ranking page title          | Your ranking · Book Smartz                                                      |

Keep the empty-state explanation when the ranking contains no completed entries.
An unfinished first comparison must remain visible separately.

### Search and book selection

| Element or condition                                       | Final copy                                                                                      |
| ---------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Search page heading                                        | Find a book to rank                                                                             |
| Search page title                                          | Find a book · Book Smartz                                                                       |
| Search results accessible label                            | Search results                                                                                  |
| Results return link                                        | Your saved ranking                                                                              |
| Search action on results page                              | Search books                                                                                    |
| Previous page action                                       | Previous page                                                                                   |
| Next page action                                           | Next page                                                                                       |
| Book selection action                                      | Select this book                                                                                |
| Selected book heading                                      | Selected book                                                                                   |
| Selected book page title                                   | Selected book · Book Smartz                                                                     |
| Selection expiry                                           | You can add this book for 15 minutes after your search. After that, search for the book again.  |
| Add action                                                 | Add to your ranking                                                                             |
| Return link                                                | Return to your ranking                                                                          |
| No matching books                                          | We found no matching books. Try another title or author.                                        |
| Results contain no books that readers can select           | We found no books you can select on this page. Try another title or author.                     |
| Search rate limit                                          | Search received too many requests. Wait a moment, then search again.                            |
| Search unavailable                                         | Search is unavailable. Try again.                                                               |
| Search timeout                                             | Search took too long to respond. Try again.                                                     |
| Invalid search response                                    | We could not read the search results. Try again.                                                |
| Search response exceeds the size limit                     | The search results were too large to load. Try again.                                           |
| Invalid query or page                                      | Enter a title or author with 1 to 256 characters. Return to your ranking to start a new search. |
| Add blocked by unfinished comparisons or an uncertain save | Finish your comparisons or check your earlier save before you add another book.                 |

The selection period starts with the search, not with the selection page.
Keep the size limit, rate limit, timeout, and unavailable conditions distinct.
The existing results form keeps its current page number when the reader changes the query.
The copy does not promise that this action returns to page 1.

### Comparisons

| Element or condition        | Final copy                                                           |
| --------------------------- | -------------------------------------------------------------------- |
| Comparison section heading  | Rank this book                                                       |
| Comparison accessible label | Book comparisons                                                     |
| New book label              | Book to rank                                                         |
| Saved book label            | Book in your ranking                                                 |
| New book summary            | Book to rank: {title}                                                |
| Question                    | Which book do you prefer?                                            |
| Choice group legend         | Choose the book you prefer                                           |
| Choice action               | Prefer {title}                                                       |
| No saved choices            | You saved no choices for this book yet.                              |
| One saved choice            | You saved 1 choice for this book.                                    |
| Multiple saved choices      | You saved {count} choices for this book.                             |
| Pause action                | Pause for now                                                        |
| Resume action               | Resume comparisons                                                   |
| Paused state                | You paused the comparisons for this book. Resume when you are ready. |
| Missing title               | Title unavailable                                                    |

Use the saved choice count only after the server confirms each choice.
Do not describe the count as the number of comparisons that remain.
A pause preserves earlier choices, but it does not give the new book a ranking position.

### Saving and confirmed results

A check inspects the saved result. Confirmation establishes that the save succeeded.

| Condition                              | Final copy                                                                  |
| -------------------------------------- | --------------------------------------------------------------------------- |
| Submission in progress                 | Saving…                                                                     |
| Book accepted, comparisons unfinished  | Book added. Compare it with books in your ranking to find its position.     |
| Choice saved, comparisons unfinished   | We saved your choice.                                                       |
| Pause confirmed                        | We paused the comparisons for this book and kept your saved choices.        |
| Resume confirmed                       | You can continue the comparisons for this book. We kept your saved choices. |
| Ranking position confirmed             | We saved this book at position {position}.                                  |
| Repeat of a saved action               | We already saved this action.                                               |
| Original save confirmed after recovery | We confirmed the original save.                                             |

Keep “Saving…” until a server response confirms the result.
Only a completed ranking entry can receive a saved position message.
A repeated action must not appear to create a second saved result.
Recovery confirmation does not imply that an unfinished book received a position.

### Uncertain saves and recovery

| Element or condition                                 | Final copy                                                                                     |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Recovery heading and action                          | Check original save                                                                            |
| Recovery explanation                                 | Check whether we saved your earlier action. This check can finish only that action.            |
| Unresolved save blocks changes                       | We cannot confirm an earlier save yet. Check the original save before you change your ranking. |
| Uncertain result after submission                    | We cannot confirm your save yet. Check the original save before you try again.                 |
| An earlier save needs checking before another Add    | Check the original save before you add another book.                                           |
| An earlier save needs checking before another choice | Check the original save before you choose another book.                                        |
| Recovery page title                                  | Check your save · Book Smartz                                                                  |
| Recovery return link                                 | Return to your ranking                                                                         |
| Recovery offers a return link without a check form   | We cannot confirm your save yet. Return to your ranking to check the original save.            |

Use the direct check instruction only when the response includes the recovery form.
Use the return instruction when the response offers only the ranking link.
The action refers to the original submitted action.
It must not create a new choice, Add, Pause, or Resume.

### Rejected or changed actions

| Condition                                | Final copy                                                                 |
| ---------------------------------------- | -------------------------------------------------------------------------- |
| Rejection page title                     | Review your ranking · Book Smartz                                          |
| Rejection heading                        | Review your ranking                                                        |
| Book already ranked                      | This book is already in your ranking.                                      |
| Unfinished comparisons block another Add | Finish the comparisons for your current book before you add another book.  |
| Ranking changed since submission         | Your ranking changed. Review it before you try again.                      |
| Comparison changed since submission      | Your comparison changed. Review the current books before you choose again. |

These messages request a fresh decision after the reader reviews the current state.
They must not suggest that an uncertain action failed.
Apply new rejection wording to future actions.
Keep historical action evidence intact.

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
