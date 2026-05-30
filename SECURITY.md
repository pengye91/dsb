# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| latest  | :white_check_mark: |

## Reporting a Vulnerability

The DSB team takes security seriously. If you discover a security vulnerability, please do **not** open a public GitHub issue.

Instead, please report it via email to the project maintainers. We will respond within 48 hours and work with you to understand and address the issue promptly.

### What to Include

- A clear description of the vulnerability
- Steps to reproduce the issue
- Affected versions/components
- Potential impact

### Disclosure Policy

We follow a coordinated disclosure process:

1. Reporter submits vulnerability privately
2. We acknowledge receipt within 48 hours
3. We investigate and develop a fix
4. We release the fix and publish an advisory
5. Credit is given to the reporter (unless anonymity is requested)

## Security Best Practices for Deployments

When deploying DSB:

1. **Always set API keys**: Use `DSB_SERVER__API_KEY` and `DSB_SERVER__ADMIN_API_KEY` with strong random values
2. **Enable authentication**: Set `require_auth: true` in your dsb.yaml
3. **Use TLS**: Deploy behind a reverse proxy (nginx, Caddy) with HTTPS
4. **Limit network exposure**: Sandboxes can execute arbitrary code — do not expose the API to the public internet without authentication
5. **Resource limits**: Configure `sandbox.default_resource_limits` to prevent resource exhaustion
6. **Regular updates**: Keep DSB and its dependencies up to date
Please report any vulnerabilities to security@dsb.dev
