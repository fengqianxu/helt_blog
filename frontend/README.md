# helt-blog frontend

This is the frontend service of the root `helt-blog` Docker Compose project.
Production runs the Vinext standalone Node.js output in the `frontend`
container, behind the Nginx `gateway`.

This directory is not an OpenAI Sites project. It intentionally has no
`.openai/hosting.json`; application data is owned by the Rust backend,
PostgreSQL, and MinIO services from the root Compose stack.

## Full Docker stack

Run these commands from the repository root:

```bash
docker compose config --quiet
docker compose up --build
```

The gateway listens on the root `.env` values `BIND_ADDRESS` and `WEB_PORT`.

For debugging with PostgreSQL and MinIO ports exposed:

```bash
docker compose -f docker-compose.yml -f docker-compose.debug.yml up --build
```

## Local frontend development

Use Node.js 22 or newer:

```bash
npm ci
npm run dev -- --host localhost --port 3000
```

The development server proxies:

- `/api` and `/health` to `API_PROXY_TARGET`, defaulting to
  `http://127.0.0.1:3001`.
- `/storage` to `STORAGE_PROXY_TARGET`, defaulting to
  `http://127.0.0.1:3000`.

If the Compose gateway already occupies port 3000, stop only the containerized
gateway and frontend while retaining the backend dependencies:

```bash
docker compose stop gateway frontend
```

Restore them with:

```bash
docker compose up -d frontend gateway
```

## Validation

```bash
npm run lint
npm test
docker build --target test -t helt-blog-frontend:test .
```

`npm test` performs a production build and verifies the rendered public and
administrator routes.
