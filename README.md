# HopsDB

[简体中文](README.zh-CN.md)

An SSH-first desktop database client built with Tauri 2, React, TypeScript, and Rust.
Connect to an SSH host, enter a database command-line client on that host, and work
with SQL results in a desktop UI. HopsDB is not a direct database-driver client.

**Status: early alpha.** Use a disposable environment and least-privileged
accounts first. Read the [security limitations](SECURITY.md) before connecting to
real systems; the current version is not recommended for sensitive production use.

## Features

- Saved SSH connections and multi-step startup commands, including nested SSH.
- Multiple independent connection workspaces and SQL tabs.
- SQL editing, completion, formatting, result-column resizing, and CSV/Excel export.
- Object browsing and source/definition tabs, with database-specific support.
- A line-oriented Command view for interacting with the remote shell.

## Database support

| Database | Remote client | Current state |
| --- | --- | --- |
| Oracle | `sqlplus` | Most established path; local integration tests are provided. |
| MySQL / MariaDB | `mysql` / compatible client | Prompt detection and HTML result parsing; object support is evolving. |
| PostgreSQL | `psql` | Prompt detection and HTML result parsing; less extensively validated. |

HopsDB detects the active database from the client's default prompt, rather than
assuming that opening an SSH connection means a database is ready. Do not customize
prompts such as `SQL>`, `mysql>`, `MariaDB [db]>`, or `db=>`; custom and continuation
prompts may prevent automatic SQL view detection. Database features are not yet at parity.

## Requirements

- Node.js 22.13+ on the 22.x line, or Node.js 24+; npm. `.nvmrc` selects Node 22.
- A current stable Rust toolchain, including Cargo.
- The [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/):
  Xcode Command Line Tools on macOS; Microsoft C++ Build Tools and WebView2 on
  Windows; WebKitGTK 4.1 and the documented system packages on Linux.
- A reachable SSH server, and the appropriate database client installed on the
  SSH host (or in a container you enter there). HopsDB does not install the server
  or remote clients for you.

macOS is the locally validated development platform. The CI workflow is configured
to check macOS, Windows, and Linux desktop compilation; this is not a guarantee of
full runtime compatibility on every platform.

## Run locally

Run these commands from this directory, which is intended to be the public
repository root:

```sh
npm ci
npm run tauri dev
```

This starts Vite on `http://localhost:1420` and opens the desktop window. The first
Rust build may take several minutes. For UI-only work, use `npm run dev`; a plain
browser cannot perform SSH, database, or desktop export operations.

## First connection

1. Create a connection and enter the SSH username, hostname, and port.
2. Add one startup command per line. These run **on the first SSH host**, not
   necessarily on the machine running the desktop app.
3. If a step needs a password, use its separate `Pwd` field instead of embedding
   credentials in command text. These fields are still persisted without
   encryption in this alpha; see [SECURITY.md](SECURITY.md).
4. Connect. A recognized database prompt opens the SQL view; otherwise inspect
   the Command view output.

Example MySQL startup command for a disposable environment:

```sh
mysql --html -u demo -p -D demo_db
```

Supply the demo database password in that line's `Pwd` field. MySQL must already
be running and accessible from the SSH host. The current MySQL result parser
expects HTML output, so include `--html` when starting the client. Do not put real
hostnames, credentials, or query data in public bug reports.

## Development and validation

```sh
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

`npm run test:watch` starts frontend tests in watch mode. Default Rust tests do
not connect to a real database; local Oracle integration and data-seeding tests
are ignored. See [the integration test guide](docs/TESTING.md) before opting in:
the seeding test drops and recreates demo tables.

The [CI workflow](.github/workflows/ci.yml) assumes this directory is the repository
root. It runs frontend validation and Rust tests, and compiles unsigned desktop
executables without installer bundles. It does not publish releases or use cloud
or database credentials.

## Build locally

```sh
npm run tauri -- build
```

Platform-specific outputs are written under `src-tauri/target/release/`; installer
bundles, when built, are under `src-tauri/target/release/bundle/`. Signing,
notarization, and public installer distribution require additional setup; no
signed official download is promised by this source repository.

## Known limitations

- SSH server keys are currently accepted without verification, and nested host-key
  prompts can be auto-confirmed. See [SECURITY.md](SECURITY.md).
- Saved connection profiles include plaintext passwords; inline command passwords
  can also appear in the Command view transcript.
- Command view is not a terminal emulator: it has no user-triggered Ctrl+C/Stop
  support and does not continuously stream output while idle.
- Startup completion uses prompt matching and short idle timeouts. Slow nested
  connections and wrappers such as `gc` / `gcloud` can cause steps to advance too
  early. Do not assume that a displayed command has completed successfully.
- The desktop webview currently has no configured Content Security Policy.
- A UI write-mode toggle is not a database authorization boundary. Use database
  permissions to enforce read-only access, and keep backups before running SQL.

## Project layout

```text
src/                       React UI and Tauri command wrappers
src/components/             Connection form, SQL editor, object tree, Command view
src-tauri/src/              Rust SSH sessions, SQL adapters, and exports
src-tauri/tests/            Optional local Oracle integration tests
.github/                    CI and contribution templates
docs/                       Testing and publication guides
```

This directory is the complete standalone application source intended for the
public repository.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) for changes and pull requests, and
[SECURITY.md](SECURITY.md) for vulnerability reporting. Maintainers preparing the
first public repository should follow [the publication checklist](docs/PUBLISHING.md).

## License

HopsDB is licensed under [Apache-2.0](LICENSE). Commercial use is permitted under
its terms. Third-party components retain their own licenses; see
[NOTICE](NOTICE) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). Apache-2.0
does not grant trademark rights or imply endorsement of a fork.
