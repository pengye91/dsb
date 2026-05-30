# DSB Node Base Image

Pre-installed npm packages for dashboard and browser tools.

## Overview

This base image pre-downloads and caches all npm packages used by the DSB dashboard and browser automation tools in sandbox images.

## Contents

### Global Packages
- **typescript** - TypeScript compiler
- **ts-node** - TypeScript execution for Node.js
- **typescript-language-server** - Language server for TypeScript IDE support

### Pre-cached Dependencies

All dashboard dependencies from `dashboard/package.json`, including:

**Core Dependencies:**
- @chakra-ui/react - UI component library
- @emotion/react, @emotion/styled - CSS-in-JS
- @xterm/* - Terminal emulator
- axios - HTTP client
- react, react-dom - React framework
- react-router-dom - Routing
- framer-motion - Animation library
- recharts - Charts
- zod - Schema validation

**Dev Dependencies:**
- vite - Build tool
- @vitejs/plugin-react - React plugin for Vite
- typescript - TypeScript compiler

## Image Details

- **Base:** `node:20-alpine`
- **Size:** ~400 MB
- **Tag:** `dsb/node-base:latest` (international), `dsb/node-base:china` (China mirrors)

## Building

### International Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/node-base:latest .
```

### China Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=true \
  --build-arg NPM_MIRROR=https://registry.npmmirror.com \
  -t dsb/node-base:china .
```

## Usage

Use in application Dockerfiles:

```dockerfile
FROM dsb/node-base:latest AS builder

WORKDIR /build
COPY package*.json ./

# Install dependencies (already cached)
RUN npm ci --legacy-peer-deps

COPY . .
RUN npm run build
```

## When to Rebuild

Rebuild this image when:
- `dashboard/package-lock.json` changes
- New npm dependencies added
- Security vulnerabilities in npm packages
- Monthly maintenance updates

## Benefits

1. **Faster builds:** 60-70% reduction in npm installation time
2. **Shared cache:** npm packages cached in Docker registry
3. **Smaller layers:** Alpine Linux keeps image size minimal

## Technical Details

### Why Alpine Linux?

Alpine Linux is used for the Node base image because:
- Minimal size (~5 MB base vs ~100 MB for Debian)
- Sufficient for Node.js applications
- Compatible with most npm packages

### Dependency Caching Strategy

The image creates a dummy package.json with all dependencies:
1. Creates temporary `package.json` with all dependencies
2. Runs `npm ci` to download and cache packages
3. Cleans up temporary files
4. Leaves cached packages in `/root/.npm`

### npm Mirror Configuration

- **International:** `https://registry.npmjs.org` (default)
- **China:** `https://registry.npmmirror.com` (Taobao mirror)

## See Also

- [Main README](../README.md)
- [Dashboard README](../../../../dashboard/README.md)
- [package.json](../../../../dashboard/package.json)
