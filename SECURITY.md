# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.3.x   | :white_check_mark: |
| < 0.3   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability in ctail, please report it responsibly:

1. **Do not** open a public GitHub issue for security vulnerabilities.
2. Use [GitHub's private vulnerability reporting](https://github.com/bisand/ctail/security/advisories/new) to submit a report. Include:
   - A description of the vulnerability
   - Steps to reproduce (if applicable)
   - Your suggested severity assessment

You can expect an initial response within 48 hours. We will work with you to understand and address the issue before any public disclosure.

> **Note:** Private vulnerability reporting must be enabled in the repository settings under **Security > Advisories**.

## Code Signing Policy

Release binaries are built from source via GitHub Actions CI/CD pipelines. The build process:

1. **Source verification** — Only code merged into the `main` branch via reviewed pull requests is built for release.
2. **Automated builds** — Release artifacts are produced by GitHub Actions workflows, ensuring reproducible builds from the public source code.
3. **No proprietary components** — All code is open source under the MIT license. No closed-source dependencies are included.
4. **Artifact integrity** — Release assets are published directly from CI to GitHub Releases with SHA-256 checksums.

## Dependencies

- **Go backend**: No external dependencies beyond the [Wails v2](https://wails.io/) framework. All internal packages (`tailer`, `config`, `rules`) use only the Go standard library.
- **Frontend**: Svelte 3 with minimal npm dependencies, compiled and embedded into the Go binary at build time.
