# Contributing to HopsDB

Issues and pull requests in English or Chinese are welcome. For security issues,
follow [SECURITY.md](SECURITY.md) instead of opening a public issue.

## Before changing code

- Check existing issues and discuss larger changes before implementing them.
- Keep changes focused. Describe which database, client, and platform are affected.
- Use synthetic connection profiles and disposable databases. Never commit real
  passwords, keys, internal hostnames, customer data, or screenshots containing them.
- Treat remote output as untrusted. Do not add raw HTML rendering or log credentials.
- Do not introduce automatic commands that change user databases during connection.

## Local development

Install the prerequisites in [README.md](README.md), then run:

```sh
npm ci
npm run tauri dev
```

Before submitting:

```sh
npm run typecheck
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
git diff --check
```

Add a regression test for behavior changes. Default tests must not depend on cloud
accounts, SSH credentials, a running database, or network access at test runtime.
Do not run the ignored integration or seeding tests against real environments;
see [docs/TESTING.md](docs/TESTING.md).

Follow the surrounding TypeScript and Rust style. Avoid unrelated whole-file
reformatting, dependency upgrades, or lockfile churn. Commit both lockfiles when
dependencies actually change.

## Pull requests

Explain the problem, the approach, and the validation you actually ran. Mention
unverified platforms and known limitations. Include sanitized screenshots for UI
changes when useful; screenshots are not a substitute for test results.

CI is configured for frontend checks and desktop compilation on Linux, Windows,
and macOS. A build does not prove a real database workflow works; document any
manual tests separately.

## Licensing contributions

By intentionally submitting a contribution for inclusion in HopsDB, you agree
that it is submitted under Apache-2.0 as described in section 5 of [LICENSE](LICENSE).
You retain copyright in your work; this is not a copyright assignment. Do not
submit code you are not entitled to license, and preserve third-party copyright,
license, and attribution notices. Update [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
when adding or changing a dependency that is shipped with the application.
