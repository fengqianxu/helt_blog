#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_root=$(dirname "$script_dir")
image_prefix=${IMAGE_PREFIX:-helt-blog}
image_tag=${IMAGE_TAG:-latest}
output_directory=${1:-release}
safe_tag=$(printf '%s' "$image_tag" | tr -c 'A-Za-z0-9_.-' '-')
bundle_directory="$project_root/$output_directory/helt-blog-$safe_tag"

cd "$project_root"
docker compose build backend frontend gateway minio-init
docker compose pull postgres minio meting artalk
mkdir -p "$bundle_directory"
cp docker-compose.yml .env.example DEPLOY.md "$bundle_directory/"
docker image save --output "$bundle_directory/images.tar" \
  "$image_prefix-frontend:$image_tag" \
  "$image_prefix-backend:$image_tag" \
  "$image_prefix-gateway:$image_tag" \
  "$image_prefix-storage-init:$image_tag" \
  postgres:16-alpine \
  minio/minio:latest \
  ghcr.io/mikus-loli/meting-api:latest \
  artalk/artalk-go:2.10.0
(cd "$bundle_directory" && sha256sum images.tar > images.tar.sha256)

printf 'Offline deployment bundle created at: %s\n' "$bundle_directory"
