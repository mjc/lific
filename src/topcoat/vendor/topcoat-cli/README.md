This directory contains the two Topcoat CLI browser modules imported by the
vendored Topcoat runtime's browser development-refresh tests. The files are
from `mjc/topcoat` commit `9c909ed4ea16b7058ae23c5e1938c83039f3e985`, rebased on
official Topcoat `main` commit `8cdc2bfd`:

- `browser/src/document.ts` checks refresh compatibility and applies document
  and region updates through the shared core morph implementation.
- `browser/src/refresh.ts` handles development refresh requests and framed
  responses.

`UPSTREAM-SHA256SUMS` records these selected fork source files before copying.
The upstream MIT license is preserved in `LICENSE`.
