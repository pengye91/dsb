# DSB Codebase Audit — 2026-05-27

## Scope

Full audit across security, code quality, dependencies/configuration, and documentation.

---

## Executive Summary

| Category | Verdict |
|----------|---------|
| Security | **Not ready** — 5 critical vulnerabilities, multiple high issues |
| Code Quality | **Not ready** — Circular deps, massive files, silent error dropping |
| Dependencies / Build | **Not ready** — Version mismatches, outdated pre-releases, Docker toolchain mismatch |
| Documentation | **Not ready** — 70-line README, 7 failing doc-tests, 31 broken intra-doc links |
| Legal / Compliance | **Ready** — SPDX headers on every file, dual license, REUSE.toml |

**Overall: Not ready for open source.** The foundation is solid but needs Phase 1 fixes before public release.

---

## 🔴 Critical Issues

### 1. DoS via Panic in File Download Handler

- **File:** `src/api/handlers/sandbox.rs:1156, 1162, 1166`
- **Code:**
  ```rust
  header::HeaderValue::from_str(&disposition).unwrap(),
  header::HeaderValue::from_str(&filename).unwrap(),
  header::HeaderValue::from_str(&sanitized_path).unwrap(),
  ```
- **Impact:** Any request with control characters (`\n`, `\r`, `\0`) in the `path` query parameter causes an immediate thread panic. `sanitize_path()` does not filter control characters.
- **Fix:** Replace `.unwrap()` with safe fallbacks or validate/sanitize for control characters before header construction.

### 2. Symlink Traversal in Static File Serving

- **File:** `src/core/static_files.rs:255, 207, 282`
- **Code:** `fs::File::open(&full_path).await?` — follows symlinks
- **Impact:** In Docker mode, the sandbox's `/public` is a host bind-mount. A sandbox user can create a symlink (`ln -s /etc/passwd /public/leak.txt`) and read arbitrary host files.
- **Fix:** Use `tokio::fs::symlink_metadata` to detect symlinks and reject them before opening files.

### 3. Command Injection in MCP Sandbox Service

- **File:** `dsb-mcp-server/src/services/sandbox.rs:445–446`
- **Code:**
  ```rust
  if let Some(dir) = &working_dir {
      shell_cmd.push_str(&format!("cd {} && ", dir)); // no escaping
  }
  shell_cmd.push_str(&command);
  let cmd = vec!["sh".to_string(), "-c".to_string(), shell_cmd];
  ```
- **Impact:** `working_dir` is supplied by the LLM/agent calling the MCP tool. Shell metacharacters (`;`, `|`, `&&`, `` ` ``, `$()`) execute arbitrary commands inside the sandbox.
- **Fix:** Apply the existing `shell_quote` helper (used in `core/static_files.rs`) to `working_dir`, or avoid shell strings entirely.

### 4. Weak Path Sanitization in Upload/Download Endpoints

- **File:** `src/api/handlers/sandbox.rs:667–683`
- **Code:**
  ```rust
  fn sanitize_path(path: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
      let sanitized = path.replace("..", "");
      let mut path = sanitized;
      while path.contains("//") {
          path = path.replace("//", "/");
      }
      if path.contains("\\") {
          return Err("Invalid path: contains dangerous characters".into());
      }
      Ok(path)
  }
  ```
- **Impact:** Only strips `..` and `//`. Does not block control characters, null bytes, or absolute paths. `../../../etc/passwd` becomes `/etc/passwd`.
- **Fix:** Use `std::path::Path::components()` to normalize, reject absolute paths, and strip/escape control characters.

### 5. Shell Command Injection in Sandbox Download

- **File:** `src/core/sandbox.rs:2229, 2241, 2255`
- **Code:**
  ```rust
  format!("test -f '{}' && echo 'exists' || echo 'notfound'", src_path)
  format!("wc -c < '{}' 2>/dev/null || echo '0'", src_path)
  format!("base64 -w0 '{}'", src_path)
  ```
- **Impact:** `src_path` is passed through the weak `sanitize_path` and interpolated into single-quoted shell strings. A single quote in the path (`/tmp/foo'bar`) breaks out of the quotes and injects shell commands.
- **Fix:** Re-use the `shell_quote` helper from `core/static_files.rs` or escape single quotes before interpolation.

### 6. Dependency Version Mismatches

| Dependency | Root / Workspace | Sub-crate | Issue |
|------------|-----------------|-----------|-------|
| `schemars` | `0.8` | MCP server `1.0` | Incompatible majors — compile two versions |
| `reqwest` | `0.13` | Agent-tester `0.12` | Compile two versions |
| `async_zip` | `0.0.18` | — | Ancient pre-release (2022), security/stability risk |

### 7. Rust Toolchain Mismatch in Docker

- **File:** `docker/base-images/rust-base/Dockerfile:25`
- **Issue:** Pins `rust:1.87.0`, but `Cargo.toml` requires `rust-version = "1.88"`.
- **Impact:** Docker builds fail.

### 8. Hardcoded AWS EFS Filesystem ID

- **File:** `deployment/helm/dsb/values.yaml:235`
- **Code:** `efs.fileSystemId: "fs-0cd02276aab388c9d"`
- **Impact:** Real AWS resource identifier committed to a public repo.

### 9. Sandbox-Base Dockerfile Security Misconfiguration

- **File:** `docker/base-images/sandbox-base/Dockerfile`
- **Issues:**
  - `echo "dsb ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/91-dsb` — passwordless sudo for ALL commands
  - `chown -h dsb:dsb /usr/bin` — unprivileged user can modify system binaries

### 10. CI Security Audit Silently Ignored

- **File:** `.github/workflows/ci.yml:209`
- **Code:** `continue-on-error: true` on `cargo audit` step
- **Impact:** Vulnerabilities never block CI / merges.

---

## 🟠 High Issues

### Architecture

| # | Issue | Details |
|---|-------|---------|
| 11 | Circular dependency: `core` → `api` | `core/types.rs`, `core/manager.rs`, `core/ssh_service.rs`, `core/sandbox.rs` all import from `api`. Correct layering: `api` → `core` → `docker`/`db`/`k8s`. |
| 12 | Silent DB error dropping | `src/db/store.rs`, `src/db/ssh_sessions.rs`, `src/db/activities.rs` use `.ok()` to swallow connection errors, query errors, and deserialization failures. |
| 13 | `unwrap()` in production paths | `src/api/server/mod.rs:214` panics on missing DB password; `src/docker/manager.rs:417,452` panic on non-UTF8 paths. |

### Code Scale

| File | Lines | Problem |
|------|-------|---------|
| `src/core/sandbox.rs` | **5,111** | Needs decomposition |
| `src/cli/commands.rs` | **4,146** | Needs split by subcommand |
| `src/docker/manager.rs` | **3,947** | Needs extraction by concern |
| `src/db/store.rs` | **3,122** | Needs split by entity |
| `src/k8s/manager.rs` | **2,906** | Also oversized |

### Other

| # | Issue | Details |
|---|-------|---------|
| 14 | `std::sync::Mutex` in async SSH gateway | `ssh-gateway/src/ssh.rs` — poisoning panics in async context. Should be `tokio::sync::Mutex`. |
| 15 | `static-server` is a skeleton | Only exports `config` module — no real functionality. |
| 16 | `Box<dyn Error>` used pervasively | `docker/manager.rs`, `core/static_files.rs`, `core/state.rs` — erases error context. |
| 17 | Dead MCP server modules | `dsb-mcp-server/src/prompts/` and `resources/` exist but are commented out in `lib.rs`. |

---

## 🟡 Medium Issues

| # | Issue | Location |
|---|-------|----------|
| 18 | 16 deprecated `bollard` API warnings | `tests/common/testcontainers_postgres.rs` |
| 19 | Unpinned GitHub Action versions | All workflows (`@stable`, `@nightly`) |
| 20 | Outdated actions | `cache@v3`, `codecov@v3`, `upload-artifact@v3` |
| 21 | `latest` tag ~20+ times in Docker | Non-reproducible builds |
| 22 | `debian:bookworm` (stable) base image | `docker/base-images/runtime-base/Dockerfile` |
| 23 | `curl ... \| sh` in Dockerfiles | Supply chain risk |
| 24 | Hardcoded UID 1000 in `libc::chown` | `src/docker/manager.rs:686` |
| 25 | `docker_trait.rs` unconventional filename | `src/docker/docker_trait.rs` |

---

## 🟢 Low Issues

| # | Issue | Location |
|---|-------|----------|
| 26 | Missing `// SAFETY:` comments on unsafe blocks | `src/docker/manager.rs`, `src/cli/commands.rs` |
| 27 | 630+ `.clone()` calls — possible optimization | Various |
| 28 | Only 4 TODOs in 71k lines of Rust | Suspiciously low — missing feature tracking |

---

## ✅ What's Actually Good

| Area | Assessment |
|------|-----------|
| No hardcoded secrets in production code | ✅ Clean |
| SQL injection prevention | ✅ All queries parameterized |
| SSRF protection | ✅ Blocks localhost, RFC 1918 |
| SPDX headers | ✅ Every file |
| Clippy cleanliness | ✅ Only 3 warnings on main crate |
| Module docs | ✅ Most `mod.rs` have `//!` docs |
| CI enforces clippy `-D warnings` | ✅ Good |
| Workspace structure | ✅ `edition = "2021"`, `resolver = "2"` |

---

## Action Plan

### Phase 1 — Security Blockers (MUST fix before open source)

1. Fix 5 critical security vulnerabilities (items 1–5)
2. Remove hardcoded AWS EFS filesystem ID (item 8)
3. Fix sandbox-base Dockerfile security (items 9a, 9b)
4. Fix dependency version mismatches (items 6, 7)
5. Fix Docker rust version mismatch (item 7)
6. Stop silently ignoring `cargo audit` failures (item 10)

### Phase 2 — Architecture & Quality

7. Break circular dependency (`core` should not import `api`)
8. Replace silent `.ok()` DB error dropping with proper propagation
9. Replace `unwrap()` on config values and paths with `?`
10. Decompose files >3000 lines
11. Replace `std::sync::Mutex` with `tokio::sync::Mutex` in ssh-gateway

### Phase 3 — Documentation & Polish

12. Rewrite README.md (install, build, test, ecosystem overview)
13. Fix 7 failing doc-tests and 31 broken intra-doc links
14. Update and pin GitHub Actions versions
15. Consolidate duplicate ARCHITECTURE.md files
