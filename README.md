# helt. 博客

项目采用全 Docker 化部署，支持 Coolify 一键构建和 Docker Compose 原生部署。应用镜像与数据服务统一编排：

- `gateway`：Nginx 统一入口，转发前端、`/api`、健康检查和公开对象
- `frontend`：Vinext/React SSR 独立生产镜像
- `backend`：Rust、axum、tokio、sqlx 独立生产镜像
- `postgres`：PostgreSQL 16，持久化业务数据
- `minio`：S3 兼容对象存储
- `minio-init`：一次性初始化公开/私有桶及访问策略

## 本地启动

```powershell
Copy-Item .env.example .env
# 编辑 .env，至少替换数据库和 MinIO 密码
docker compose up -d --build
docker compose ps
```

Coolify 请直接选择仓库根目录的 `docker-compose.coolify.yml`，只需给 `gateway` 绑定域名并设置 `PUBLIC_ORIGIN`。完整步骤、生产安全配置与离线部署说明见 [DEPLOY.md](DEPLOY.md)。

访问地址：

- 网站：<http://localhost:8080/>
- API：<http://localhost:8080/api/v1>
- 就绪检查：<http://localhost:8080/health/ready>
- MinIO 控制台：<http://localhost:9001/>

后端会在启动时自动执行 sqlx 迁移、插入默认配置，并在管理员表为空时创建首个管理员。`ADMIN_INITIAL_PASSWORD` 留空时，随机密码只在首次创建时输出一次：

```powershell
docker compose logs backend | Select-String "initial administrator"
```

若首次启动日志已经丢失，可在容器内生成并打印一个新密码：

```powershell
docker compose exec backend blog-admin reset-password
```

验证服务：

```powershell
Invoke-RestMethod http://localhost:8080/health/live
Invoke-RestMethod http://localhost:8080/health/ready
Invoke-RestMethod http://localhost:8080/api/v1
docker compose exec postgres psql -U helt -d helt_blog -c "\dt"
```

## 构建与测试镜像

```powershell
# 构建全部应用镜像
docker compose build backend frontend gateway

# 前端构建、HTML 测试和 ESLint
docker build --target test -t helt-blog-frontend-test ./frontend

# Rust 单元测试与 Clippy
docker build --target test -t helt-blog-backend-test ./backend
```

后端当前处于接口契约阶段：72 个产品端点均已登记，尚未实现的处理器返回标准 JSON `501 Not Implemented`。可执行契约位于 `backend/src/routes/contract.rs`，全局标准、业务边界和逐接口 TDD 用例见 [技术文档/05-后端接口标准与TDD.md](技术文档/05-后端接口标准与TDD.md)。

## 常用命令

```powershell
docker compose logs -f gateway frontend backend
docker compose up -d --build frontend backend gateway
docker compose down
```

PostgreSQL 和 MinIO 端口默认只绑定宿主机回环地址；后端与前端应用端口只存在于 Compose 内部网络，对外统一使用 `WEB_PORT`。

完整的 Coolify、离线打包、镜像仓库和生产部署说明见 [DEPLOY.md](DEPLOY.md)。迁移文件位于 `backend/migrations/`，会编译进后端二进制；新增迁移后重新构建后端镜像即可。
