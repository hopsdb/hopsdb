# Security policy

## Current status and scope

HopsDB is early-alpha software. Security work targets the current development
version; no older-release maintenance or response-time guarantee is offered.
Do not use this version for sensitive production connections without reviewing
and addressing the limitations below.

## Known limitations

- The first SSH connection accepts any server host key. It does not yet enforce
  `known_hosts` validation or ask the user to approve a stored fingerprint.
- Nested SSH host-key prompts can be automatically answered `yes` during startup.
- SSH and per-command passwords are stored in plaintext in the desktop webview's
  `localStorage`, along with saved command text. Password-input masking is not
  encryption. Do not save credentials you cannot afford to expose locally.
- Command text is shown verbatim in the session transcript. Inline passwords may
  be exposed by screenshots, copying output, local storage, or the remote shell's
  own history. Separate `Pwd` fields reduce inline exposure, but remain plaintext
  at rest in this version.
- The desktop webview currently has no configured Content Security Policy.
- Command view has no user-triggered Ctrl+C/Stop. Prompt-based startup detection
  may advance early for slow connections and unrecognized wrapper commands.
- SQL executes with the remote account's permissions. The UI write-mode toggle
  is not a sandbox or a replacement for database-side authorization.

Use trusted networks, disposable databases, and least-privileged accounts. Do not
run the application as an administrator unless you have a specific reason. Back
up data before running SQL, and do not treat an SSH connection as proof that the
intended database session is ready.

## Reporting a vulnerability

Use the repository's **Security → Advisories → Report a vulnerability** option
when GitHub private vulnerability reporting is enabled. Include the affected
version, platform, impact, and a minimal reproduction using synthetic data.

If that option is unavailable, use a private contact channel published by the
maintainer on their GitHub profile. If none is available, ask the maintainer to
enable a private reporting channel without disclosing the vulnerability in the
request. Do not post exploit details, credentials, database contents, or private
connection profiles in a public issue.

If a credential was disclosed, revoke or rotate it with the issuing service.
Removing a file, screenshot, or commit does not make the credential safe again.

Maintainers must enable and test a private reporting channel before making the
repository public; see [docs/PUBLISHING.md](docs/PUBLISHING.md).
