# Unsigned WASM candidate envelope v2

This evolves `integrationer.verified-wasm-candidate.v1` into
`integrationer.verified-wasm-candidate.v2`, profile `connector` or
`conformance_fixture`. Tool version: **0.1.0**. It is not a third parallel
connector-specific format. The old diagnostic and echo v1 profiles remain
fixture/test-only and are not accepted by this validator. Today's Integrationer
staging endpoint still accepts only its pinned v1 fixture, **not these v2 bundles**.

The public Python 3.11+ CLI/library uses only the standard library. It neither
builds nor executes WASM, reads CI variables, contacts URLs, imports platform
models, nor authorizes publication. Source revisions and build/test references
are opaque strings, not Git/GitHub requirements. Pin the tool's source commit;
no package release or installation is required.

## Files and integrity rules

Exactly four ordinary files in a directory (no archives, links or extra files):

| File | Limit | Meaning |
|---|---:|---|
| connector.wasm | 16 MiB | Exact tested bytes, component header required |
| contract.json | 1 MiB | Reported WIT version, complete/partial coverage, named action/trigger JSON input/output schemas |
| tests.json | 1 MiB | Passed nonempty named checks, source/build/test references, byte/contract hashes, producer-specific details |
| manifest.json | 64 KiB | v2 profile, producer assertion, source identity, raw hashes/sizes, immutable false authority flags |

All hashes are lowercase SHA-256 over **raw file bytes**, including JSON whitespace
and trailing newlines. There is no canonicalization. JSON must be UTF-8 without
BOM, duplicate keys, nonfinite numbers or unpaired surrogates; nesting is limited
to 32. Integers allow at most 128 decimal digits; fractional/exponent numbers
must be finite binary64 values. Identity/reference strings contain 1–512 characters
and no ASCII control characters. The writer emits sorted UTF-8 JSON as a convenience, not a verification
rule. Manifest/contract/test/check structural fields are exact; JSON Schema bodies
and test details are extensible objects. No archive paths or URL fetching exist.
`vectors/valid-external` is a byte-for-byte example. `vectors/cases.json` defines
language-neutral mutations/expected outcomes for future server conformance tests.

`artifact.wit`, contract schemas, producer/source/build references and passing
checks are **producer assertions**. Offline verification checks their shape and
bindings, not actual exported WIT behavior, schema completeness, test honesty,
trusted origin, or release approval. A WASM header check is not a full binary
validation. Optional expected WASM/WIT and a separately received manifest hash
can reject substituted identities; they do not create a signature. All authority
flags (`origin_verified`, `publication_authorized`, `published`) must remain false.
A conformance fixture must never be admitted as an executable connector.

## CI-independent use

Produce `contract.json` and `tests.json` from your own build/test system. Tests
must bind `artifact_sha256`, `contract_sha256`, `wit`, `source`, and
`build_reference` to exactly the candidate being packaged. No runner type is
required; the CLI rejects absent/failed checks but cannot authenticate them.

```sh
python3 candidate/candidate.py package --wasm /path/to/built.wasm \
  --contract /path/to/contract.json --tests /path/to/tests.json \
  --producer urn:example:builder --source-reference urn:example:source \
  --revision revision-42 --build-reference urn:example:build:12 --output bundle
python3 candidate/candidate.py verify bundle --expected-revision revision-42 \
  --expected-wasm "$INDEPENDENT_WASM_SHA256" --expected-wit 5.0.0 \
  --expected-manifest "$INDEPENDENT_MANIFEST_SHA256"
```

Select expected values independently; copying them out of the same untrusted
manifest adds no identity assurance. The CLI exits nonzero on invalid input and
never replaces an existing output directory.

## Optional RSpec/AppBridge adapter

`adapters/rspec.py` is separate from the format/library. It accepts an explicit
already-built WASM path and explicit producer/source/build/test arguments. It
extracts selected schemas through real AppBridge with an empty app environment
and the repository's existing synthetic `TestHelper.base_connection`, then runs
the entire existing RSpec suite with `CONNECTOR_WASM_PATH`. It refuses failed,
pending or empty suites and changed bytes. Its output contains the actual RSpec
results, reported WIT version and partial schema coverage. It does not build,
contact live accounts or create credentials. Repo specs must honor that path.

The adapter **executes trusted repository code/WASM** in a disposable test runner;
it is not a sandbox for outside submissions and must not run in server requests.
Selected schema functions must be safe with synthetic connection data. The core
`verify` command is inert. No Ruby/Rust/RSpec dependency exists in the core format.

A repository config contains only `test_helper`, `actions`, and `triggers`:

```json
{"test_helper":"spec/test_helper.rb","actions":["example"],"triggers":[]}
```

Run the shared adapter with `--config` and `--wasm`, `--output`, `--producer`,
`--source-reference`, `--revision`, `--build-reference`, `--test-reference`.
Repository CI remains responsible for lint/build and for passing authoritative
context. Use the same pinned tools commit for producer and offline consumer.

## Conformance and limits of this delivery

```sh
python3 -m unittest discover -s candidate -p 'test_*.py'
env -i /absolute/path/to/python3 candidate/make_fixture.py /tmp/external-bundle
python3 candidate/candidate.py verify /tmp/external-bundle --expected-revision revision-42
```

The external fixture has an empty component and tests **envelope shape only**. It
uses no GitHub variables or Standout producer identity. It is not evidence of
an executable third-party connector or implemented WIT exports. It remains
`conformance_fixture`, unverified and unauthorized.

The next server slice must reuse these vectors, keep parsing separate from
fixture trust/test gates, and enforce an independent authenticated
producer-to-app policy (including A-to-B denial). A producer string in this
unsigned envelope cannot establish that identity. No production trust policy,
signature key, version import, promotion or public release is activated here.
