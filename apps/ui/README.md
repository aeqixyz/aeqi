# sigil-ui

Web dashboard for the Sigil AI agent orchestrator.
Canonical source now lives in the Sigil monorepo at `apps/ui`.

**Tech stack:** Vite 6 + React 19 + TypeScript 5 + Zustand + React Router v7

## Quick Start

```bash
npm --prefix apps/ui install
npm --prefix apps/ui run dev
```

Or from inside this directory:

```bash
cd apps/ui
npm install
npm run dev      # Dev server on http://localhost:5173
npm run build    # Production build to dist/
```

## API

The dev server proxies `/api/*` requests to the `sigil-web` backend on `localhost:8400`.
In production, `sigil-web` can serve `dist/` directly when `[web].ui_dist_dir` is configured. A thin reverse proxy such as `nginx` or `caddy` can sit in front for TLS and host routing.

## Backend

Sigil monorepo: <https://github.com/0xAEQI/sigil>
