This directory contains the Topcoat core browser modules imported by the
vendored Topcoat runtime's source tests and browser build inputs. The files are
from `mjc/topcoat` commit `8c3e6adee4694b504e6df13c90428666100ea167`, based on
official Topcoat `main` commit `341f3ff2`:

- `browser/dev.ts` defines the development refresh event and runtime contract.
- `browser/frames.ts` defines the shared framed-render protocol.
- `browser/morph.ts` implements the current DOM reconciliation behavior.

`UPSTREAM-SHA256SUMS` records these selected fork source files before copying.
The upstream MIT license is preserved in `LICENSE`.
