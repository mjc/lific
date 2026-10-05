# Project archive import

This module mounts the archive import flow at `/projects/import`. Project creation
and project administration are owned by the native Topcoat views.

Load `SCRIPT` and `STYLESHEET` once after the shared session runtime. The runtime
mounts `[data-topcoat-project-settings="archive"]`; `attach(root, {session, win})`
also supports explicit mounting and returns `controller`, `refresh`, and `dispose`.

The importer checks the signed-in account's archive capability before showing the
upload form. Uploads use the private same-origin multipart endpoint with the
current session bearer. Progress and server-side processing are reported
separately. Responses are discarded after account or token changes. Unknown
outcomes survive reload in account-specific session storage and block repeat
imports until the user checks the project list.

A successful import displays the imported row and file counts, unresolved external
references, and a link to the new project. Invalid archives and definite server
refusals can be retried. Large archives must be imported with the CLI.
