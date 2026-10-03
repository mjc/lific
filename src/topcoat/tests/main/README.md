# Frontend tests from main

The baseline is `9683d38af8e1e6f9b076439fe90d9519109b2218`, shared by both master remotes when the port started. It contains 34 frontend unit/helper sources and 14 browser scripts/helpers. `baseline.json` records their Git blob IDs and named test declarations.

Each group keeps its source-to-test mapping in `inventory.json`. The inventory test checks all 48 sources and original static case names. Independent review checks assertion equivalence, parameterized cases, and browser helpers; matching names alone does not establish coverage.

## Run

```sh
devenv --profile topcoat tasks run lific:topcoat:main-test
devenv --profile e2e tasks run lific:topcoat:main-e2e
```

The browser task builds the executable and installs browser dependencies through the existing workflow. Browsers run headless. Tests use separate scratch databases; archive round trips use two real instances.

Inside the project environment, the runner also accepts `unit`, `browser`, or `all`, followed by an optional JSONL report path:

```sh
node src/topcoat/tests/main/run.js unit /tmp/frontend-unit-results.jsonl
node src/topcoat/tests/main/run.js browser /tmp/frontend-browser-results.jsonl
```

Every file runs even when an earlier file fails. Failures retain a nonzero exit status and run in CI. Product assertions stay active; selector, setup, and adapter mistakes are corrected during the port. Missing adapters are incomplete coverage and are reported separately from confirmed product failures.

Confirmed failures and their reproduction commands belong in the single [failure ledger](https://lific.mjc.lol/LIF/issues/LIF-228).
