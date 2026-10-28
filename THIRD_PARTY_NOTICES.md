# Third-party components

HopsDB's own code is licensed under Apache-2.0. Dependencies remain under their
respective licenses; the project's LICENSE does not relicense third-party code.
Copyright statements and full license terms in the upstream distributions take
precedence over this inventory.

## Direct frontend runtime dependencies

The following licenses were checked in the installed package metadata corresponding
to `package-lock.json` when this inventory was prepared. Version numbers below
are the locked versions, not the broader ranges in `package.json`.

| Package | Version | Declared license |
| --- | --- | --- |
| `@codemirror/autocomplete` | 6.20.3 | MIT |
| `@codemirror/commands` | 6.11.0 | MIT |
| `@codemirror/lang-sql` | 6.10.0 | MIT |
| `@codemirror/state` | 6.7.1 | MIT |
| `@codemirror/view` | 6.43.9 | MIT |
| `@tanstack/react-table` | 8.21.3 | MIT |
| `@tauri-apps/api` | 2.11.1 | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-opener` | 2.5.4 | MIT OR Apache-2.0 |
| `@uiw/react-codemirror` | 4.25.11 | MIT |
| `react` | 19.2.8 | MIT |
| `react-dom` | 19.2.8 | MIT |
| `sql-formatter` | 15.8.2 | MIT |

Upstream license texts are distributed in the installed package directories,
typically as `LICENSE`, `LICENSE_MIT`, or `LICENSE_APACHE-2.0`. Transitive dependencies
and build tools are not fully enumerated in this table.

## Rust dependencies

The desktop backend uses Tauri, the opener plugin, serde, serde_json,
rust_xlsxwriter, dirs, futures-util, russh, shlex, tokio, bytes, and tauri-build,
plus their transitive dependencies. `src-tauri/Cargo.lock` records resolved
versions. Generate the full dependency metadata for the release environment with:

```sh
cargo metadata --manifest-path src-tauri/Cargo.toml --locked --format-version 1
```

Review each shipped component's license expression and its upstream license and
NOTICE files. This document is an inventory starting point, **not a complete
binary-redistribution notice bundle or a completed license audit**.

## Before distributing binaries

- Enumerate all components actually included in the frontend bundle and the
  target-specific Rust executable, including transitive dependencies.
- Collect and distribute the required copyright statements, full license texts,
  and applicable NOTICE contents. A package-name table alone is not sufficient.
- Review bundled native libraries and installer/runtime redistribution terms for
  each target platform.
- Review icons and other assets independently; absence of a package dependency
  does not establish that an asset may be redistributed.
- Recheck this inventory after dependency or asset changes. Dependency license
  metadata is useful evidence, not a substitute for reviewing upstream terms.
