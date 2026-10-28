# Testing HopsDB

Run commands from the standalone repository root (the directory containing
`package.json` and `src-tauri/`).

## Default validation

After installing the platform prerequisites and running `npm ci`:

```sh
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

Frontend tests use Vitest with jsdom selected in the test files. `npm test` runs
once; `npm run test:watch` is for interactive development. Tests mock the desktop
API and do not establish real SSH connections.

Rust unit tests cover prompt detection and SQL parsing/helpers. The three local
Oracle integration tests are marked `#[ignore]` and do not run by default.
Passing these tests does not verify live SSH, database interoperability, the
operating system credential store, or packaged installer behavior.

The CI workflow also compiles a desktop executable on each configured platform:

```sh
npm run tauri -- build --debug --no-bundle
```

This is a compilation check, not a signed installer release or a desktop UI test.

## Optional Oracle integration tests

These tests assume an existing **disposable local Oracle environment**. They do
not create it, and a self-contained provisioning configuration is not currently
included. Before running them, review all three files under `src-tauri/tests/`.

The current fixtures assume:

- An SSH service on `127.0.0.1:2222` with the synthetic `oracle` test account.
- A nested SSH service reachable as `oracle@127.0.0.1` from that environment.
- `sqlplus` at `/opt/oracle/product/26ai/dbhomeFree/bin`.
- An Oracle service reachable as `localhost:1521/FREEPDB1` inside the SSH host.
- The hard-coded passwords in those test files are disposable demo credentials,
  not real environment credentials. Never reuse them for an exposed service.

The tests use a privileged demo database account. Confirm that the SSH endpoint
is your isolated fixture, not a forwarded port to another system.

Run only the intended test by name:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test e2e_oracle -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test sql_oracle -- --ignored --nocapture
```

The separate seeding test **drops and recreates** `HOPS_*` demo tables and inserts
sample data. Do not execute it in a shared, staging, or production database:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test seed_demo_oracle -- --ignored --nocapture
```

Do not run a blanket `cargo test -- --ignored`: it also selects the destructive
seeding test. These tests can print connection commands and query output; do not
publish their logs without checking and sanitizing them.

## Adding regression coverage

- Prefer small unit tests that do not require cloud accounts or live databases.
- Use synthetic credentials and data; never record real session transcripts.
- For prompt handling, include delayed output, continuation prompts, launch
  errors, and nested-session transitions rather than only a happy-path prompt.
- Document database-client and server versions for manual interoperability tests.
- Keep environment-dependent tests explicitly opt-in and explain their effects.
