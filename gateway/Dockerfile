FROM nginx:alpine

COPY nginx.conf /etc/nginx/nginx.conf
RUN cp /etc/nginx/nginx.conf /tmp/nginx.conf \
    && sed -i -e 's/server frontend:3000;/server 127.0.0.1:3000;/' \
              -e 's/server backend:3000;/server 127.0.0.1:3000;/' \
              -e 's/server artalk:23366;/server 127.0.0.1:23366;/' \
              -e 's/server minio:9000;/server 127.0.0.1:9000;/' /etc/nginx/nginx.conf \
    && nginx -t \
    && mv /tmp/nginx.conf /etc/nginx/nginx.conf

EXPOSE 80
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=6 \
  CMD wget --quiet --output-document=- http://127.0.0.1/health/ready >/dev/null || exit 1

CMD ["nginx", "-g", "daemon off;"]
