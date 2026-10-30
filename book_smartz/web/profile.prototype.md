# Private reader profile prototype

This throwaway prototype supports [Design the private profile page and editing states](https://github.com/LowkeyLab/bazel-repo/issues/2842).
Do not merge this branch into the app.
The maintainer must review the prototype before the decision closes.

## Open the prototype

Open `profile.prototype.html` in a browser from this checkout.
The adjacent `fonts` directory supplies `Literata` and `Source Sans 3`.
No server, account, or database is necessary.

Use the bottom controls to compare these layouts:

- `?variant=A`: a centered bookplate, then a stacked form.
- `?variant=B`: a wide bookplate, then two editing columns on desktop.
- `?variant=C`: a full-width blue bookplate, then a narrower stacked form.

Each layout shows the saved name first, then the biography and ranked-book count.
The editing controls follow the saved profile.
Phone layouts use one column.
Arrow keys change the layout when focus is outside an interactive control.
Changing layouts preserves the current draft.

## Design plan

| Element           | Choice                 |
| ----------------- | ---------------------- |
| Background        | Library blue `#f1f5fb` |
| Profile surface   | White `#ffffff`        |
| Text              | Ink `#18243a`          |
| Actions           | Blue `#315bc7`         |
| Save confirmation | Green `#216b52`        |
| Heading font      | `Literata`             |
| Control font      | `Source Sans 3`        |
| Alignment         | Left aligned           |

The bookplate gives the saved profile its own reading area.
The form follows it. Only the bookplate shows the saved values.
All variants retain the existing palette and fonts.
The variants compare width, editing columns, and the bookplate treatment.
They preserve the maintainer's approved content order.

## Approved behavior

The maintainer approved these choices in the `Wayfinder` discussion:

1. Show the saved profile first. Keep the form visible after it.
2. Use explicit **Save changes** and **Discard changes** controls.
3. Update the saved profile only after save confirmation.
4. Preserve entered text after validation errors and definite save failures.
5. On a conflicting edit, reload the latest saved profile. Discard local edits.
6. Preserve the draft in the current tab during sign-in recovery for the same reader.
7. Require an explicit save after sign-in recovery. Clear the draft on sign-out.
8. Use **Your books** for the ranking destination, consistent with the current copy guide.

The existing map also requires optional plain-text fields, no display-name limit, and a 500-character biography limit.
Only books with a saved rank contribute to the count.
Unfinished placements contribute nothing.

## Review the states

Use **Prototype controls** to load an example or select the next save result.

1. Load the empty example. Check the zero count and optional fields.
2. Load the imported-name example. Edit either field. Select **Save changes**.
3. Select the rejected-save result. Check that the entered text remains.
4. Load the invalid biography. Save it. Check the message and field focus.
5. Select the conflicting-edit result. Save a local change. Check that the newer saved profile replaces it.
6. Select the expired-session result. Save a change. Select **Sign in with Google**. Review the draft before saving again.
7. Select the unconfirmed-save result. Check the save before another edit.
8. Load the long-text and non-Latin examples. Compare all layouts at phone width.
9. Use the keyboard to reach every control. Check visible focus and status messages.
10. Sign out with an unsaved draft. Check that the draft clears.

## Prototype limits

The prototype keeps all state in memory.
Its conflict response simulates a reload without a real network request.
Its sign-in control simulates a return for the same reader.
It does not implement Google authentication or retention across a real sign-in redirect.
Reloading or closing the prototype clears its draft.
The production implementation must verify reader identity before restoring any retained draft.

The unconfirmed-save example resolves to a confirmed save when the reader checks it.
It illustrates a review proposal. It does not define the production persistence protocol.
The biography counter counts Unicode code points. The maintainer must confirm the final character-count rule.
The count and navigation use fixtures. They do not read a real ranking.
The navigation links do not leave the prototype.

No prototype state establishes implementation, accessibility, provider, or deployment acceptance.
