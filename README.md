# helt. 博客

这是一个完全运行在 Docker 容器中的博客项目。日常启动不需要在宿主机安装
Node.js、Rust、PostgreSQL 或 MinIO，只需要 Docker Engine 和 Docker Compose v2。

## 服务组成

| 服务 | 用途 | 宿主机默认入口 |
| --- | --- | --- |
| `gateway` | Nginx 统一入口，转发页面、API、健康检查和公开文件 | `127.0.0.1:3000` |
| `frontend` | Vinext/React SSR 前端 | 仅容器网络 |
| `backend` | Rust/Axum API；启动时执行数据库迁移 | `127.0.0.1:3001`，仅供本机调试 |
| `postgres` | PostgreSQL 16 业务数据库 | `127.0.0.1:5432` |
| `minio` | S3 兼容对象存储 | API `127.0.0.1:9000`，控制台 `127.0.0.1:9001` |
| `minio-init` | 创建公开/私有桶并设置访问策略 | 一次性任务，成功后退出 |

浏览器和正式客户端只应访问 `gateway`。容器之间使用 Compose 服务名通信，
不要把 `localhost` 写成容器内的 PostgreSQL 或 MinIO 地址。

## 首次启动

### 1. 检查环境

```powershell
docker --version
docker compose version
```

建议为 Docker 分配至少 4 GB 内存。确认宿主机的 `3000`、`3001`、`5432`、
`9000` 和 `9001` 端口未被占用；也可以在下一步的 `.env` 中修改对应端口。

### 2. 创建配置

Windows PowerShell：

```powershell
Copy-Item .env.example .env
```

Linux/macOS：

```bash
cp .env.example .env
```

编辑 `.env`，至少替换以下三项，不能保留示例值：

```dotenv
POSTGRES_PASSWORD=使用足够长的URL安全随机值
MINIO_ROOT_PASSWORD=使用足够长的随机值
AUTH_JWT_SECRET=至少32个字符的随机值
```

数据库密码会嵌入连接 URL，推荐只使用字母、数字、`-` 和 `_`，不要使用
`@`、`:`、`/`、`#`。本机默认访问地址无需修改：

```dotenv
PUBLIC_ORIGIN=http://localhost:3000
CORS_ALLOWED_ORIGINS=http://localhost:3000
```

生产环境应把这两项改成实际 HTTPS 域名。

### 3. 构建并启动

```powershell
docker compose config --quiet
docker compose up -d --build
docker compose ps
```

首次构建需要下载基础镜像和依赖，耗时通常比后续启动长。`postgres` 和
`minio` 健康后，`minio-init` 会创建桶，随后后端执行 SQL 迁移；前后端健康后
`gateway` 才会启动。`minio-init` 显示 `Exited (0)` 是正常状态。

所有长期运行服务都应显示 `Up ... (healthy)`。启动完成后访问：

- 网站：<http://localhost:3000/>
- API：<http://localhost:3000/api/v1>
- 就绪检查：<http://localhost:3000/health/ready>
- MinIO 控制台：<http://localhost:9001/>

### 4. 获取初始管理员密码

`ADMIN_INITIAL_PASSWORD` 留空时，后端只在首次创建管理员时生成并输出一次密码：

```powershell
docker compose logs backend | Select-String "initial administrator"
```

Linux/macOS 使用：

```bash
docker compose logs backend | grep "initial administrator"
```

如果日志已丢失，可重置密码：

```powershell
docker compose exec backend blog-admin reset-password
```

## 日常使用

```powershell
# 启动已有容器
docker compose up -d

# 查看状态和日志
docker compose ps
docker compose logs -f gateway frontend backend postgres minio

# 代码或 Dockerfile 变化后重新构建
docker compose up -d --build

# 仅重建应用层，不影响数据服务
docker compose up -d --build frontend backend gateway

# 停止并删除容器，保留数据库和对象文件
docker compose down
```

不要在有数据需要保留时执行 `docker compose down -v`，它会删除 PostgreSQL 和
MinIO 的命名卷。只修改 `.env` 后也要再次执行 `docker compose up -d`，Compose
才会按新配置重建相关容器。

## 验证与排错

```powershell
Invoke-RestMethod http://localhost:3000/health/live
Invoke-RestMethod http://localhost:3000/health/ready
docker compose exec postgres psql -U helt -d helt_blog -c "\dt"
docker compose logs --tail 200 backend
```

常见问题：

- `port is already allocated`：修改 `.env` 中冲突的 `WEB_PORT`、`BACKEND_PORT`、
  `POSTGRES_PORT`、`MINIO_API_PORT` 或 `MINIO_CONSOLE_PORT`。
- `backend` 反复重启：先查看后端日志，并检查密码、`AUTH_JWT_SECRET` 长度和
  PostgreSQL 健康状态。
- `gateway` 没有启动：它会等待前端和后端健康；分别查看这两个服务的日志。
- 修改数据库用户名、数据库名或密码不会自动改写已有 PostgreSQL 数据卷中的
  账户。已有环境请在数据库内迁移账户，或仅在确认不需要数据后重建卷。

## 测试镜像

```powershell
docker build --target test -t helt-blog-frontend-test ./frontend
docker build --target test -t helt-blog-backend-test ./backend
```

前端测试镜像执行构建、HTML 测试和 ESLint；后端测试镜像执行 Rust 单元测试和
Clippy。后端未实现的契约接口会返回标准 JSON `501 Not Implemented`。

生产部署、Coolify、离线打包和镜像仓库部署见 [DEPLOY.md](DEPLOY.md)。
