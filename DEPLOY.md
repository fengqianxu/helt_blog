# 部署说明

项目提供两个部署入口：

- `docker-compose.coolify.yml`：用于 Coolify，不发布任何宿主机端口，由 Coolify 代理统一接入。
- `docker-compose.yml`：用于 Docker Engine + 官方 Compose 插件，默认只监听宿主机 `127.0.0.1:18080`。
- `docker-compose.debug.yml`：可选的本机调试覆盖文件，显式开放后端、数据库和 MinIO；生产环境不加载。

两种方式都会启动网关、前端、后端、Artalk、PostgreSQL 与 MinIO；数据库迁移和对象桶初始化会在首次启动时自动完成。

## Coolify（推荐）

要求 Coolify `v4.0.0-beta.411` 或更高版本，以支持 Compose 的随机密码环境变量。

先将项目（包括 `docker-compose.coolify.yml`）提交并推送到 Coolify 能访问的 Git 仓库，然后：

1. 在 Coolify 中新建 **Application**，选择该 Git 仓库和 **Docker Compose** Build Pack。
2. Base Directory 设为 `/`，Docker Compose Location 设为 `/docker-compose.coolify.yml`。
3. 在 `gateway` 服务上填写站点域名，例如 `https://blog.example.com`；容器端口是 `80`，域名后不需要追加端口。
4. 在环境变量中只需填写：

   ```dotenv
   PUBLIC_ORIGIN=https://blog.example.com
   ```

   普通 Docker Compose 部署还需把 `.env` 中的 `AUTH_JWT_SECRET` 替换为至少 32
   字符的随机值；Coolify 会通过 `SERVICE_PASSWORD_64_JWT` 自动生成该密钥。

   `SERVICE_PASSWORD_64_LLM` 会作为独立的 LLM 凭据加密密钥生成并持久化，不与
   JWT Secret 共用。已有部署如果曾使用 JWT 回退，请按 README 的双密钥窗口步骤
   将旧 JWT Secret 作为版本 1 previous key，完成一次原子轮换。

   如果需要允许其他前端域名跨域访问，再设置逗号分隔的 `CORS_ALLOWED_ORIGINS`；默认与 `PUBLIC_ORIGIN` 相同。
5. 点击 Deploy。不要给 `frontend`、`backend`、`artalk`、`postgres` 或 `minio` 分配域名。

Coolify 会自动生成并持久保存以下密码，无需手工创建：

- `SERVICE_PASSWORD_64_POSTGRES`
- `SERVICE_PASSWORD_64_MINIO`
- `SERVICE_PASSWORD_64_ADMIN`
- `SERVICE_PASSWORD_64_ARTALK`
- `SERVICE_PASSWORD_64_ARTALK_ADMIN`
- `SERVICE_PASSWORD_64_LLM`

首个后台管理员用户名默认为 `helt`（可通过 `ADMIN_USERNAME` 修改），初始密码就是 Coolify 环境变量 `SERVICE_PASSWORD_64_ADMIN` 的值。

评论控制台位于 `https://blog.example.com/artalk/`，登录名为 `blog-service`、邮箱为
`artalk-service@localhost.invalid`，密码是 Coolify 生成的
`SERVICE_PASSWORD_64_ARTALK_ADMIN`。该账户也供后端同步文章的禁止评论与删除状态。
默认情况下访客评论需审核；如需直接公开，在 Coolify 环境变量中设置
`ARTALK_PENDING_DEFAULT=false` 并重新部署。

部署完成后访问：

```text
https://blog.example.com/
https://blog.example.com/health/ready
```

Coolify 配置不发布任何宿主机端口，并使用入口 `edge` 网络，以及 `web`、
`api`、`database` 和 `storage` 四个隔离的内部网络。持久化数据保存在 `postgres_data`、
`minio_data` 和 `artalk_data` 命名卷中。`minio-init` 是正常执行后退出的一次性任务，已从
Coolify 总体健康检查中排除。

> `exclude_from_hc` 是 Coolify 的 Compose 扩展字段，因此不要用原生 `docker compose` 运行 `docker-compose.coolify.yml`；原生部署请使用默认的 `docker-compose.yml`。

## Docker Engine / Docker Compose

要求 Docker Engine 和 Docker Compose v2 插件。完整的本地首次启动与排错步骤见
[README.md](README.md)。服务器首次部署：

```bash
cp .env.example .env
# 编辑 .env：替换 POSTGRES_PASSWORD、MINIO_ROOT_PASSWORD、AUTH_JWT_SECRET、
# LLM_ENCRYPTION_KEY、ARTALK_APP_KEY、ARTALK_ADMIN_PASSWORD，
# 并按实际 HTTPS 地址设置 PUBLIC_ORIGIN 和 CORS_ALLOWED_ORIGINS。
docker compose config --quiet
docker compose up -d --build
docker compose ps
curl http://127.0.0.1:18080/health/ready
```

数据库密码会嵌入 PostgreSQL URL，请使用足够长的 URL 安全字符组合（字母、数字、`-`、`_`），不要直接使用 `@`、`:`、`/`、`#` 等字符。

生产环境推荐保持 `BIND_ADDRESS=127.0.0.1`，由宿主机的 HTTPS 反向代理转发到 `WEB_PORT`。如果容器必须直接监听公网地址，将其改为 `BIND_ADDRESS=0.0.0.0`，并自行配置防火墙和 TLS。

标准部署只发布 `WEB_PORT`，不要在生产环境叠加
`docker-compose.debug.yml`。PostgreSQL、后端和 MinIO 通过内部网络及服务名
通信，不依赖宿主机端口，因此不会与宿主机上同类服务发生端口冲突。

当 `ADMIN_INITIAL_PASSWORD` 留空时，后端会在首次创建管理员时生成随机密码并只打印一次：

```bash
docker compose logs backend | grep "initial administrator"
```

## 日常操作

```bash
docker compose ps
docker compose logs -f gateway frontend backend artalk
docker compose up -d --build
docker compose down
```

`docker compose down` 会保留命名卷。生产环境不要执行 `docker compose down -v`，该命令会删除数据库与对象文件。

仅修改 `.env` 后也应运行 `docker compose up -d`，让 Compose 重建配置发生变化的
容器。修改 PostgreSQL 的数据库名、用户名或密码不会自动更新已有数据卷中的账户。

## 离线部署

Windows 打包：

```powershell
.\scripts\package-images.ps1 -Tag 1.0.0
```

Linux/macOS 打包：

```bash
IMAGE_TAG=1.0.0 ./scripts/package-images.sh
```

将生成的 `release/helt-blog-<tag>/` 复制到目标主机，然后执行：

```bash
sha256sum -c images.tar.sha256
docker load -i images.tar
cp .env.example .env
# 修改 .env，并保持 IMAGE_TAG 与打包标签一致
docker compose up -d --no-build
```

## 镜像仓库部署

```bash
export IMAGE_PREFIX=registry.example.com/team/helt-blog
export IMAGE_TAG=1.0.0
docker compose build backend frontend gateway
docker compose push backend frontend gateway
```

目标主机设置相同的 `IMAGE_PREFIX` 和 `IMAGE_TAG` 后执行：

```bash
docker compose pull
docker compose up -d --no-build
```
