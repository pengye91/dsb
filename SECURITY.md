# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| latest  | :white_check_mark: |
| < latest | :x:                |

We follow a rolling-release model: only the latest commit on `main` receives security fixes. Older releases are not patched.

## Reporting a Vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Use one of these channels (in order of preference):

### 1. GitHub Security Advisories (recommended)

Open a [private security advisory](https://github.com/pengye91/dsb/security/advisories/new) on this repository. This is the only channel that:

- Keeps the report private between you and the maintainers until a fix is published
- Allows the maintainers to credit you in the published advisory (unless you request anonymity)
- Triggers GitHub's native CVE / GHSA workflow
- Lets us coordinate a fix, release, and disclosure timeline inside GitHub

### 2. Email

If you cannot or prefer not to use GitHub Advisories, email the maintainer at the address listed on the [@pengye91 GitHub profile](https://github.com/pengye91).

We will acknowledge receipt within **48 hours** and work with you to understand and address the issue promptly.

### What to Include

- A clear description of the vulnerability
- Steps to reproduce the issue
- Affected versions / components
- Potential impact (data exposure, RCE, privilege escalation, etc.)
- Any workarounds you've identified

## Disclosure Policy

We follow a coordinated disclosure process:

1. Reporter submits vulnerability privately (via GitHub Advisory or email)
2. We acknowledge receipt within 48 hours
3. We investigate and develop a fix
4. We release the fix and publish an advisory
5. Credit is given to the reporter (unless anonymity is requested)

We aim to publish a fix and advisory within **30 days** of confirmation for high-severity issues, and within **90 days** for lower-severity issues. We'll coordinate the disclosure date with you.

## Security Best Practices for Deployments

When deploying DSB:

1. **Always set API keys**: Use `DSB_SERVER__API_KEY` and `DSB_SERVER__ADMIN_API_KEY` with strong random values (e.g. `openssl rand -hex 32`).
2. **Enable authentication**: Set `require_auth: true` in your `dsb.yaml` (it defaults to enabled).
3. **Use TLS**: Deploy behind a reverse proxy (nginx, Caddy, Traefik) with HTTPS.
4. **Limit network exposure**: Sandboxes can execute arbitrary code — do not expose the API to the public internet without authentication and rate limiting.
5. **Resource limits**: Configure `sandbox.default_resource_limits` to prevent resource exhaustion from runaway sandboxes.
6. **Docker socket is root-equivalent**: Mounting `/var/run/docker.sock` into the DSB container gives it effective root on the Docker host. Run DSB on a dedicated host/VM, not a shared one. Consider rootless Docker for higher isolation.
7. **Regular updates**: Keep DSB and its dependencies up to date.
8. **Audit logs**: If you enable PostgreSQL persistence, the `sandbox_activities` table records every API action — review it periodically.
