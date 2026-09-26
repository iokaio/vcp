# Saved-view implementation note

Revision sv-34, recorded 2026-09-13.

The server implements create, list, load, rename, and delete with an ownership check on each route. A successful create returns the saved columns, filters, and sort order. Conflicting names and the five-view limit are checked before mutation. Delete removes one selected view; there is no undo or restore path. On network failure during save or rename, the UI keeps the editor contents and shows a retry message; it does not display a success toast.

The client supports keyboard access to the view picker, save dialog, and rename dialog. An implementation note is not accessibility validation. A signed-out browser should contain no cached saved-view payload after sign-out, but browser verification remains open.

Review responsibilities: Mira owns server/API checks; Leon owns browser behavior and keyboard checks; Priya owns the final release evidence review. Reviewers may use two synthetic user accounts in an isolated test environment when it becomes available. No real user data or credentials belong in the plan.
