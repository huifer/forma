# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| 0.2.x   | ✅ |
| < 0.2   | ❌ |

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Use GitHub's **private vulnerability reporting** (Security tab → Report a vulnerability), or
email the maintainers at **[INSERT SECURITY CONTACT — e.g. security@yourdomain.com]**.

Include:

- the affected component (`crates/forma/src/...`, the CLI, or the MCP server) and version
- a minimal reproduction (spec files + command)
- your assessment of severity and impact
- any workaround you've identified

## Scope

Forma is a local build tool. Areas we treat as security-relevant:

- **MCP stdio server** (`forma mcp`): input validation of JSON-RPC frames; tool arguments must not enable path traversal outside the project directory.
- **Patch engine**: transactional integrity (a failed patch must not leave partially-written specs); text-editing primitives must not corrupt spec files beyond their documented behavior.
- **Generated code**: generators must not emit code that executes untrusted input unexpectedly (e.g. unsanitized identifiers interpolated into generated TypeScript).
- **Manifest parsing**: hostile `forma.yaml` / spec files must not panic the engine or escape the project directory.

Out of scope: the example project's generated application code (it is a demo with TODO handlers), and the confidentiality of spec files themselves (Forma is a local tool — it does not exfiltrate anything, but it also does not encrypt).

## Response targets

- Acknowledgment: within 72 hours
- Triage + severity: within 7 days
- Fix or mitigation for high-severity issues: within 30 days

We credit reporters in the release notes unless they prefer to remain anonymous.
