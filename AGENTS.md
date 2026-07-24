# Codex project instructions

## Project type

This repository is a Docker Compose application, not an OpenAI Sites project.
Do not create `.openai/hosting.json`, use Sites deployment, or assume Cloudflare
hosting unless the user explicitly asks to migrate the application to Sites.

The Compose project name is `helt-blog` and is declared in the root
`docker-compose.yml`.

## Services and source layout

- `gateway/`: Nginx entry point exposed on `${BIND_ADDRESS}:${WEB_PORT}`.
- `frontend/`: Vinext/React frontend packaged as a standalone Node.js server.
- `backend/`: Rust/Axum API, PostgreSQL migrations, authentication, and MinIO
  object-storage integration.
- PostgreSQL and MinIO are managed by the root Compose project.
- `docker-compose.debug.yml` exposes local debugging ports.
- `docker-compose.coolify.yml` is the production/Coolify override.

Run Docker Compose commands from the repository root so `.env` and all override
files resolve correctly.

## Common commands

```powershell
docker compose config --quiet
docker compose up --build
docker compose down
```

For local debugging with exposed database and storage ports:

```powershell
docker compose -f docker-compose.yml -f docker-compose.debug.yml up --build
```

For the production/Coolify configuration:

```powershell
docker compose -f docker-compose.yml -f docker-compose.coolify.yml config --quiet
```

## Validation

Before handing off changes, run the checks relevant to the files changed:

```powershell
Set-Location frontend
npm run lint
npm test

Set-Location ..\backend
cargo fmt --all -- --check
cargo test
```

When the local Windows Rust toolchain lacks the MSVC linker, run backend tests
inside the Rust Docker image or build the backend Docker target instead. For
database migration changes, validate all migrations in order against a
temporary PostgreSQL 16 container.

Never commit `.env`, credentials, generated `frontend/dist`, or `backend/target`.
