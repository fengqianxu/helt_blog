FROM minio/mc:latest

COPY frontend/public/saber-day.png /seed/raiments/saber/cover.png
COPY frontend/public/saber-night.png /seed/raiments/alter-saber/cover.png
COPY storage-init/seed/ /seed/
COPY storage-init/init.sh /usr/local/bin/init-storage

ENTRYPOINT ["/bin/sh", "/usr/local/bin/init-storage"]
