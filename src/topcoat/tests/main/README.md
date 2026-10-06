# Frontend tests from main

Pinned main is `9683d38af8e1e6f9b076439fe90d9519109b2218`. `baseline.json`
records the 48 original frontend test/helper sources and named assertions.
Inventories retain their forward mappings; a mapping is not a coverage pass.

The intermediate application JavaScript and controller fixtures are deleted.
Native Rust is the implementation, and pinned main is the behavioral reference.
Unported features have no hybrid fallback. The original assertions stay in this
suite; adapters awaiting native subjects report `ERR_NATIVE_PORT_MISSING` rather
than loading a retired controller. Those errors are incomplete test coverage,
not demonstrated product failures. Actual native browser assertions remain
active, including the missing Comments section. No case is skipped.

Run through the repository workflow:

```sh
devenv --profile topcoat tasks run lific:topcoat:main-test
devenv --profile e2e tasks run lific:topcoat:main-e2e
```

The runner supports `unit`, `browser`, and `all`, with an optional JSONL report:

```sh
node src/topcoat/tests/main/run.js unit /tmp/frontend-unit-results.jsonl
node src/topcoat/tests/main/run.js browser /tmp/frontend-browser-results.jsonl
```

Browser fixtures use real servers and scratch databases; browsers run headless.
Missing native functionality, unproven helper contracts, and adapter gaps remain
separate in `failure-ledger.json`. Its older counts are historical checkpoints.
See `issues/comments-native-coverage.md` for the partial comment translation.
