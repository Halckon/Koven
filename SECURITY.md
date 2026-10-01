# Security Policy

## Supported Versions

Only the current main development branch and the latest released tags receive security updates.

| Version | Supported          |
| ------- | ------------------ |
| main    | :white_check_mark: |
| < 0.1   | :x:                |

---

## Reporting a Vulnerability

The Koven team takes security issues seriously. If you believe you have discovered a vulnerability (such as a memory safety loophole in compiler lowering, unintended code execution, or compiler crash under malicious inputs), please report it responsibly.

### How to Report

- **Email**: Send details to [halckon0@gmail.com](mailto:halckon0@gmail.com).
- **GitHub Advisory**: Alternatively, submit a private report via the **Security Advisory** tab on GitHub:
  `https://github.com/Halckon/koven/security/advisories/new`

### What to Include

Please provide:
1. A clear description of the vulnerability.
2. Minimal reproducible `.ko` source code and compiler flags.
3. Your operating system, architecture, and toolchain version (`rustc --version`, `llvm-config --version`).
4. Any potential mitigations or impact analysis you have identified.

### Response Timeline

- We will acknowledge receipt of your vulnerability report within 48 hours.
- We will coordinate a remediation timeline and keep you informed.
- Please do not disclose vulnerabilities publicly until a fix has been released.
