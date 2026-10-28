# Publishing the standalone repository

The public repository should start with the contents of this application
directory at its root. Do not publish the parent development repository or its
Git history. Copy only this application directory; do not include sibling
private folders from a parent monorepo.

## Before the first push

- [ ] Confirm ownership/permission for all published code and assets, and
  preserve applicable third-party notices.
- [ ] Include `LICENSE`, `NOTICE`, both README files, and the contribution/security
  documents. Check assets and dependency notices independently.
- [ ] Copy only intended source and documentation into a new directory outside the
  old repository. Do not copy `.git`, `node_modules`, `dist`, `src-tauri/target`,
  generated schemas, local editor state, credentials, query exports, or logs.
- [ ] Scan **the files to be published**, not just source text, for secrets and
  private infrastructure details. Review screenshots and binary assets manually.
  A fresh history avoids publishing old commits; it does not sanitize current files.
- [ ] Review `.gitignore`, then initialize a new Git repository in the clean
  directory. Inspect `git status` and the staged file list before committing.
- [ ] Do not reuse the old repository's remote or push its branches/tags. Start
  the new repository with its own initial commit and `main` branch.
- [ ] From the clean copy, run `npm ci`, `npm test`, `npm run build`, and the default
  Rust tests. The CI workflow expects `package.json` at the new repository root.
- [ ] Create the GitHub repository privately first; do not generate conflicting
  README/license files in GitHub's creation wizard.

## GitHub settings before making it public

- [ ] Enable GitHub private vulnerability reporting and verify that the security
  reporting option is accessible. Do not invent a maintainer email in SECURITY.md.
- [ ] Add the actual repository URL to package metadata/documentation where useful.
- [ ] Enable dependency alerts and the secret-scanning/push-protection features
  available for the repository.
- [ ] Run the first CI workflow and inspect all three desktop platforms. Workflows
  that lived under a nested app directory in a parent monorepo will not run
  until `.github` is at the new repository root.
- [ ] Protect the default branch with review and CI requirements appropriate to
  the project. Keep workflow permissions minimal and do not expose release
  credentials to pull-request workflows.
- [ ] Publish only sanitized screenshots of synthetic data. Label the project an
  early alpha and keep known security limitations visible.
- [ ] Resolve the security limitations before advertising production readiness.
  In particular: SSH host identity checks, credential storage, transcript
  redaction, webview CSP, and interactive-command cancellation/readiness.

## Before distributing an installer

- [ ] Review runtime behavior on each target platform, not just compilation.
- [ ] Generate the complete target-specific third-party license/NOTICE bundle;
  `THIRD_PARTY_NOTICES.md` alone is not sufficient for binary redistribution.
- [ ] Add signing/notarization where appropriate, keeping signing secrets out of
  source control and pull-request jobs. Do not tell users to disable OS security
  protections as the standard installation method.
- [ ] Include changes, known issues, platform/architecture support, checksums, and
  license information in the release. Keep version numbers synchronized across
  npm metadata, Rust metadata, and Tauri configuration.
- [ ] Mark alpha releases as prereleases. Publish checksums and installable assets
  only after verifying the actual generated artifacts.

The supplied CI only validates and compiles; it never creates a GitHub release,
uploads installers, changes visibility, or provisions a database.
