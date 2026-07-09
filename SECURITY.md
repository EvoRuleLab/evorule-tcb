# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: Active development |
| < 0.1.0 | :x: Not supported  |

Only the latest minor release receives security updates. Earlier versions are not maintained.

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, please report them by email to:

**evorulelab@gmail.com**

You should receive a response within **7 days**. If for some reason you do not, please follow up via email to ensure we received your original message.

Please include the following information (as much as you can provide) to help us better understand the nature and scope of the possible issue:

- Type of issue (e.g., buffer overflow, SQL injection, cross-site scripting, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- The location of the affected source code (tag/branch/commit or direct URL)
- Any special configuration required to reproduce the issue
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

This information will help us triage your report more quickly.

## Response Process

1. **Acknowledgment** - We will acknowledge receipt of your report within 7 days.
2. **Triage** - We will investigate and determine impact, severity, and affected versions.
3. **Fix** - We will develop a fix and prepare a security advisory.
4. **Disclosure** - We will coordinate the disclosure timing with you. We aim to release fixes within 30 days for high-severity issues.
5. **CVE** - For high-severity issues, we will request a CVE identifier through GitHub Security Advisories.

## Scope

This security policy applies to:

- The `evorule-tcb` Rust crate published to crates.io
- The source code in this repository
- Documentation that describes how to use the crate securely

It does **not** apply to:

- Downstream applications that use `evorule-tcb` (those have their own security policies)
- The reference EvoRule governance layer (separate project, separate policy)

## Security Properties

`evorule-tcb` is designed with the following security properties in mind:

- **Determinism**: Same input + same rules = same output, forever. This means predictability for audit purposes.
- **Auditability**: Every state transition is recorded in an append-only hash chain (`audit.rs`).
- **Immutability**: TCB state is immutable (R-07 paradigm gate enforced).
- **Isolation**: No `unsafe` (G-01 enforced). No external process execution (R-08 enforced).

For details, see `docs/spec/en-US/TCB_Governance_Contract.md` and `docs/spec/en-US/EvoRule_Determinism_Standard.md`.

## Acknowledgments

We would like to thank the following individuals and organizations for responsibly disclosing security issues:

*(No reports yet - this list will be updated as issues are disclosed and fixed.)*

## Out of Scope

The following are generally not considered security vulnerabilities:

- Issues requiring physical access to a user's machine
- Issues requiring the user to install malicious software
- Theoretical issues without a concrete attack scenario
- Issues in dependencies that have not yet been patched upstream (we track these but the fix must come from the upstream maintainer)
- Issues in test code (`#[cfg(test)]` modules)
