This directory contains the Topcoat core browser modules imported by the
vendored Topcoat runtime's source tests and browser build inputs. The files are
from `mjc/topcoat` commit `9c909ed4ea16b7058ae23c5e1938c83039f3e985`, rebased on
official Topcoat `main` commit `8cdc2bfd`:

- `browser/dev.ts` defines the development refresh event and runtime contract.
- `browser/frames.ts` defines the shared framed-render protocol.
- `browser/morph.ts` implements the current DOM reconciliation behavior.

`UPSTREAM-SHA256SUMS` records these selected fork source files before copying.
The upstream MIT license is preserved in `LICENSE`.
