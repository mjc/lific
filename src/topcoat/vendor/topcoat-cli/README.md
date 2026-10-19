This directory contains the two Topcoat CLI browser modules imported by the
vendored Topcoat runtime's browser development-refresh tests. The files are
from `mjc/topcoat` commit `8c3e6adee4694b504e6df13c90428666100ea167`, based on
official Topcoat `main` commit `341f3ff2`:

- `browser/src/document.ts` checks refresh compatibility and applies document
  and region updates through the shared core morph implementation.
- `browser/src/refresh.ts` handles development refresh requests and framed
  responses.

`UPSTREAM-SHA256SUMS` records these selected fork source files before copying.
The upstream MIT license is preserved in `LICENSE`.
