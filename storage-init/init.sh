#!/bin/sh
set -eu

: "${MINIO_ROOT_USER:?MINIO_ROOT_USER is required}"
: "${MINIO_ROOT_PASSWORD:?MINIO_ROOT_PASSWORD is required}"

until mc alias set local http://minio:9000 "${MINIO_ROOT_USER}" "${MINIO_ROOT_PASSWORD}"; do
  sleep 2
done

mc mb --ignore-existing local/blog-public
mc mb --ignore-existing local/blog-private
mc anonymous set download local/blog-public
mc anonymous set none local/blog-private

copy_if_missing() {
  source_path=$1
  object_key=$2
  mc stat "local/blog-public/${object_key}" >/dev/null 2>&1 \
    || mc cp "${source_path}" "local/blog-public/${object_key}"
}

copy_if_missing /seed/raiments/saber/cover.png raiments/saber/cover.png
copy_if_missing /seed/raiments/alter-saber/cover.png raiments/alter-saber/cover.png
copy_if_missing /seed/voice/login/alter-saber-success.mp3 voice/login/alter-saber-success.mp3
copy_if_missing /seed/voice/login/alter-saber.mp3 voice/login/alter-saber.mp3
copy_if_missing /seed/voice/login/blue-saber-success.mp3 voice/login/blue-saber-success.mp3
copy_if_missing /seed/voice/login/blue-saber.mp3 voice/login/blue-saber.mp3
copy_if_missing /seed/avatars/default/admin-avatar.webp avatars/default/admin-avatar.webp
