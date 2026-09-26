# Accepted scope: saved dashboard views

Decision SV-12, accepted 2026-09-11 by the Larch maintainers.

A saved view stores the signed-in user's chosen columns, sort order, and filters for one dashboard. It never stores dashboard row data. A user may keep at most five views per dashboard. View names are trimmed, must contain 1 to 40 Unicode scalar values, and must be unique case-insensitively within that user's views on that dashboard. The same name may be used on a different dashboard.

Saving and renaming need a successful server response before the UI confirms success. Saving a sixth view or a duplicate name must leave the existing views unchanged and show an actionable error. Loading an unknown or inaccessible view must reveal no stored fields and return the user to the dashboard's default view with an explanatory message.

Private ownership is the release boundary: another user cannot list, load, rename, or delete a user's views. Signing out clears the client view cache. Existing default dashboards remain usable without saved views.

Deferred: team sharing, public links, and restoring deleted views. This acceptance plan must not make those features a release requirement or describe them as delivered.
