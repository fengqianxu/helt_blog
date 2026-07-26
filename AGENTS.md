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

This repository is validated as a Docker Compose project. Run the commands from
the repository root rather than validating frontend and backend with host
toolchains directly:

```powershell
# Validate the base Compose model and the production/Coolify override.
docker compose config --quiet
docker compose -f docker-compose.yml -f docker-compose.coolify.yml config --quiet

# Build the service test stages. These stages run the frontend lint/render
# checks and the backend cargo test/clippy checks inside their Docker images.
docker build --target test -t helt-blog-frontend-test ./frontend
docker build --target test -t helt-blog-backend-test ./backend

# For changes crossing service boundaries, run a Compose smoke test.
docker compose up --build -d
docker compose ps
docker compose down
```

For database migration changes, start the Compose PostgreSQL 16 service and
validate all migrations in order against that container. Use the debug override
when host access to PostgreSQL or MinIO is required:

```powershell
docker compose -f docker-compose.yml -f docker-compose.debug.yml up --build -d
docker compose -f docker-compose.yml -f docker-compose.debug.yml ps
docker compose -f docker-compose.yml -f docker-compose.debug.yml down
```

Do not rely on a host Windows Rust linker or host Node installation as the
project validation path; the Docker test stages are the source of truth.

Never commit `.env`, credentials, generated `frontend/dist`, or `backend/target`.
