//! 后端 HTTP 契约目录与未实现路由骨架。
//!
//! 本模块是 TDD 的第一层防线：先固定每个端点的方法、路径、认证方式、
//! 成功状态码、请求/响应形状和核心业务规则，再逐个把占位处理器替换为真实实现。
//! 在业务处理器完成前，已登记端点统一返回结构化 `501 Not Implemented`；这能让前端
//! 区分“契约已存在但尚未实现”和“路径写错导致 404”。

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, OriginalUri},
    http::{Method, StatusCode},
    response::IntoResponse,
    routing::{MethodFilter, on},
};

use crate::{
    error::{ErrorBody, ErrorEnvelope},
    state::AppState,
};

/// 普通 JSON/表单请求的默认上限。大文件上传必须在契约中显式使用素材上限。
pub const DEFAULT_REQUEST_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;

/// 素材库单文件上限，与最新设计稿的 200 MB 要求一致。
pub const ASSET_UPLOAD_LIMIT_BYTES: usize = 200 * 1024 * 1024;

/// 契约支持的 HTTP 方法。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// 返回日志、文档和测试断言使用的标准大写方法名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    /// 转为 axum 注册路由所需的方法过滤器。
    const fn filter(self) -> MethodFilter {
        match self {
            Self::Get => MethodFilter::GET,
            Self::Post => MethodFilter::POST,
            Self::Put => MethodFilter::PUT,
            Self::Patch => MethodFilter::PATCH,
            Self::Delete => MethodFilter::DELETE,
        }
    }

    /// 转为测试请求使用的 `http::Method`。
    #[cfg(test)]
    pub fn http_method(self) -> Method {
        match self {
            Self::Get => Method::GET,
            Self::Post => Method::POST,
            Self::Put => Method::PUT,
            Self::Patch => Method::PATCH,
            Self::Delete => Method::DELETE,
        }
    }
}

/// 访问端点所需的会话凭据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Authentication {
    /// 无需登录；写操作仍必须执行限流和输入校验。
    Anonymous,
    /// 只读取 httpOnly refresh cookie，用于轮换后台会话。
    RefreshCookie,
    /// 需要有效的后台 JWT httpOnly cookie。
    AdminJwt,
}

/// 单个 HTTP 端点的可执行契约。
///
/// `request`、`response` 和 `business_rule` 不只是说明文字：契约完整性测试会检查
/// 每项均已填写；实现阶段的单元/集成测试应直接以这些字段为验收依据。
#[derive(Clone, Copy, Debug)]
pub struct EndpointContract {
    /// 稳定测试编号；重命名路径时编号不变，便于追踪需求和回归失败。
    pub id: &'static str,
    /// 业务域，用于测试分组和模块拆分。
    pub domain: &'static str,
    /// HTTP 方法。
    pub method: HttpMethod,
    /// axum 路径模板，动态参数使用 `{name}`。
    pub path: &'static str,
    /// 不含模板参数的代表性测试路径。
    pub example_path: &'static str,
    /// 认证要求。
    pub authentication: Authentication,
    /// 正常完成时的标准状态码；异步任务统一为 202，创建统一为 201。
    pub success_status: StatusCode,
    /// 对外用途摘要。
    pub summary: &'static str,
    /// 查询、请求体、Cookie 或 multipart 的输入契约。
    pub request: &'static str,
    /// 成功响应体及关键响应头契约。
    pub response: &'static str,
    /// 状态转换、过滤范围、副作用和主要失败分支。
    pub business_rule: &'static str,
    /// 路由允许读取的最大请求体；普通接口 2 MB，素材上传 200 MB。
    pub max_body_bytes: usize,
}

macro_rules! endpoint_with_limit {
    ($id:literal, $domain:literal, $method:ident, $path:literal, $example:literal,
     $auth:ident, $status:ident, $summary:literal, $request:literal,
     $response:literal, $rule:literal, $limit:expr) => {
        EndpointContract {
            id: $id,
            domain: $domain,
            method: HttpMethod::$method,
            path: $path,
            example_path: $example,
            authentication: Authentication::$auth,
            success_status: StatusCode::$status,
            summary: $summary,
            request: $request,
            response: $response,
            business_rule: $rule,
            max_body_bytes: $limit,
        }
    };
}

macro_rules! endpoint {
    ($id:literal, $domain:literal, $method:ident, $path:literal, $example:literal,
     $auth:ident, $status:ident, $summary:literal, $request:literal,
     $response:literal, $rule:literal) => {
        endpoint_with_limit!(
            $id,
            $domain,
            $method,
            $path,
            $example,
            $auth,
            $status,
            $summary,
            $request,
            $response,
            $rule,
            DEFAULT_REQUEST_BODY_LIMIT_BYTES
        )
    };
}

macro_rules! asset_upload_endpoint {
    ($id:literal, $domain:literal, $method:ident, $path:literal, $example:literal,
     $auth:ident, $status:ident, $summary:literal, $request:literal,
     $response:literal, $rule:literal) => {
        endpoint_with_limit!(
            $id,
            $domain,
            $method,
            $path,
            $example,
            $auth,
            $status,
            $summary,
            $request,
            $response,
            $rule,
            ASSET_UPLOAD_LIMIT_BYTES
        )
    };
}

/// v1 的全部 106 个业务端点。
///
/// 健康检查和 API 索引属于运维入口，不计入产品端点总数。数组顺序按“认证 → 公开
/// 业务 → 后台业务”排列，与 `技术文档/03-API接口总表.md` 保持一致。
pub static ENDPOINT_CONTRACTS: &[EndpointContract] = &[
    // 认证域：登录类端点匿名可用，其余后台端点必须先通过会话校验。
    endpoint!(
        "AUTH-01",
        "认证",
        Post,
        "/api/v1/admin/auth/login",
        "/api/v1/admin/auth/login",
        Anonymous,
        OK,
        "账号密码登录",
        "JSON: username、password、remember，已开启 TOTP 时必须提供 totp_code",
        "200 JSON admin{username}；设置 2 小时 access cookie，remember=true 时再设置 7 天 refresh cookie",
        "用户名、密码或 TOTP 任一错误均返回同一 401，禁止泄露账号是否存在；连续失败需要限流"
    ),
    endpoint!(
        "AUTH-02",
        "认证",
        Post,
        "/api/v1/admin/auth/passkey/options",
        "/api/v1/admin/auth/passkey/options",
        Anonymous,
        OK,
        "获取 Passkey 登录挑战",
        "无请求体；服务端按当前 RP ID 生成一次性 WebAuthn challenge",
        "200 JSON challenge、rp_id、allow_credentials、timeout",
        "挑战必须短期有效且一次性消费；没有 Passkey 时仍返回不可用于伪造账号枚举的安全响应"
    ),
    endpoint!(
        "AUTH-03",
        "认证",
        Post,
        "/api/v1/admin/auth/passkey/verify",
        "/api/v1/admin/auth/passkey/verify",
        Anonymous,
        OK,
        "校验 Passkey 断言并登录",
        "JSON: WebAuthn assertion，必须关联 AUTH-02 尚未消费的 challenge",
        "200 JSON admin{username} 并设置 access cookie",
        "校验 origin、RP ID、签名和 sign_count；挑战过期、重放或签名错误统一返回 401"
    ),
    endpoint!(
        "AUTH-04",
        "认证",
        Post,
        "/api/v1/admin/auth/refresh",
        "/api/v1/admin/auth/refresh",
        RefreshCookie,
        OK,
        "静默续签后台会话",
        "无请求体；读取 httpOnly refresh cookie",
        "200 并轮换 access/refresh cookie，不在 JSON 中暴露令牌",
        "refresh token 必须单次轮换；缺失、过期、已吊销或重放返回 401 并清除无效 cookie"
    ),
    endpoint!(
        "AUTH-05",
        "认证",
        Post,
        "/api/v1/admin/auth/logout",
        "/api/v1/admin/auth/logout",
        AdminJwt,
        NO_CONTENT,
        "退出后台会话",
        "无请求体；携带 access cookie，可选携带 refresh cookie",
        "204 无响应体并清除两类认证 cookie",
        "存在 refresh token 时必须吊销；重复退出保持幂等，不暴露令牌状态"
    ),
    endpoint!(
        "AUTH-06",
        "认证",
        Get,
        "/api/v1/admin/auth/me",
        "/api/v1/admin/auth/me",
        AdminJwt,
        OK,
        "读取当前管理员信息",
        "无请求体；读取 access cookie",
        "200 JSON username、role、email、avatar_url、bilibili_uid",
        "只返回当前会话用户的账户中心资料；无效或过期会话返回 401"
    ),
    endpoint!(
        "AUTH-07",
        "认证",
        Get,
        "/api/v1/admin/auth/passkeys",
        "/api/v1/admin/auth/passkeys",
        AdminJwt,
        OK,
        "列出已绑定 Passkey",
        "无查询参数",
        "200 JSON items[{id,label,created_at}]，不返回公钥或 credential 原文",
        "仅列出当前管理员的凭据，按 created_at 倒序"
    ),
    endpoint!(
        "AUTH-08",
        "认证",
        Post,
        "/api/v1/admin/auth/passkeys/options",
        "/api/v1/admin/auth/passkeys/options",
        AdminJwt,
        OK,
        "获取 Passkey 注册挑战",
        "无请求体；当前管理员必须已登录",
        "200 JSON challenge、rp、user、pub_key_cred_params、exclude_credentials",
        "challenge 与当前管理员绑定、短期有效且一次性消费；已有 credential 放入排除列表"
    ),
    endpoint!(
        "AUTH-09",
        "认证",
        Post,
        "/api/v1/admin/auth/passkeys",
        "/api/v1/admin/auth/passkeys",
        AdminJwt,
        CREATED,
        "绑定新的 Passkey",
        "JSON: WebAuthn attestation、可选 label；challenge 来自 AUTH-08",
        "201 JSON id、label、created_at",
        "校验 attestation 后原子写入；重复 credential 返回 409，挑战失效返回 422"
    ),
    endpoint!(
        "AUTH-10",
        "认证",
        Delete,
        "/api/v1/admin/auth/passkeys/{id}",
        "/api/v1/admin/auth/passkeys/1",
        AdminJwt,
        NO_CONTENT,
        "移除一个 Passkey",
        "路径参数 id 为正整数",
        "204 无响应体",
        "只能删除当前管理员凭据；目标不存在返回 404，是否允许删除最后一个凭据由密码登录能力兜底"
    ),
    endpoint!(
        "AUTH-11",
        "认证",
        Post,
        "/api/v1/admin/auth/forgot-password",
        "/api/v1/admin/auth/forgot-password",
        Anonymous,
        ACCEPTED,
        "登记忘记密码请求",
        "JSON: username",
        "202 无敏感数据；可返回通用提示",
        "无论用户名是否存在都返回相同结果；v1 仅记录安全事件并提示使用 blog-admin CLI 重置"
    ),
    endpoint!(
        "AUTH-12",
        "认证",
        Post,
        "/api/v1/admin/auth/change-password",
        "/api/v1/admin/auth/change-password",
        AdminJwt,
        NO_CONTENT,
        "修改当前管理员密码",
        "JSON: current_password、new_password；新密码长度为 12–128 个字符",
        "204 无响应体并清除当前认证 cookie",
        "必须复核当前密码；更新 Argon2id 哈希后吊销全部 refresh token，并要求当前会话重新登录"
    ),
    endpoint!(
        "AUTH-13",
        "认证",
        Patch,
        "/api/v1/admin/auth/profile",
        "/api/v1/admin/auth/profile",
        AdminJwt,
        OK,
        "更新当前管理员个人资料",
        "JSON: email、bilibili_uid；头像资源必须通过 AUTH-14 单独上传",
        "200 JSON username、role、email、avatar_url、bilibili_uid",
        "只更新当前管理员；邮箱与 UID 服务端校验，Bilibili UID 写入 bangumi_sync 配置并从站点设置界面迁入账户中心"
    ),
    endpoint!(
        "AUTH-14",
        "认证",
        Post,
        "/api/v1/admin/auth/avatar",
        "/api/v1/admin/auth/avatar",
        AdminJwt,
        OK,
        "上传或替换当前管理员头像",
        "请求体为 PNG、JPEG 或 WebP 原始字节，Content-Type 必须匹配，最大 512 KB",
        "200 JSON username、role、email、avatar_url、bilibili_uid；avatar_url 为 /storage/ 下的 MinIO 资源",
        "服务端校验文件签名后写入公共 MinIO 桶，并原子创建 uploads/asset_versions 记录；后续替换沿用同一逻辑资源追加版本"
    ),
    endpoint!(
        "AUTH-15",
        "认证",
        Delete,
        "/api/v1/admin/auth/avatar",
        "/api/v1/admin/auth/avatar",
        AdminJwt,
        OK,
        "移除当前管理员头像绑定",
        "无请求体",
        "200 JSON username、role、email、avatar_url=null、bilibili_uid",
        "只解除当前管理员头像并归档逻辑资源，历史 MinIO 版本继续由素材库管理"
    ),
    endpoint!(
        "PROFILE-01",
        "公开资料",
        Get,
        "/api/v1/profile",
        "/api/v1/profile",
        Anonymous,
        OK,
        "读取站点作者的公开资料",
        "无查询参数",
        "200 JSON username、email、avatar_url",
        "与账户中心保存的头像和邮箱使用同一数据源；不公开登录凭据、角色或 Bilibili UID"
    ),
    // 文章与分类域：公开查询只允许读取 published 内容。
    endpoint!(
        "ARTICLE-01",
        "文章",
        Get,
        "/api/v1/articles",
        "/api/v1/articles?page=1&per_page=10",
        Anonymous,
        OK,
        "查询已发布文章列表",
        "Query: page>=1、per_page=1..50、category/tag 使用 slug、可选 group_by=year",
        "200 分页 items；group_by=year 时 items 为 year/count/items 分组",
        "仅返回 published；置顶优先再按 published_at 倒序；无效分页或 group_by 返回 422"
    ),
    endpoint!(
        "ARTICLE-02",
        "文章",
        Get,
        "/api/v1/articles/{slug}",
        "/api/v1/articles/p1",
        Anonymous,
        OK,
        "读取已发布文章详情",
        "路径参数 slug 非空且符合站点 slug 规则",
        "200 返回正文、标签、上下篇、相关文章和 allow_comment",
        "只读取 published；成功读取后浏览量加一；不存在或草稿返回 404，相关文章按同分类/标签计算"
    ),
    endpoint!(
        "TAXONOMY-01",
        "分类标签",
        Get,
        "/api/v1/categories",
        "/api/v1/categories",
        Anonymous,
        OK,
        "读取分类及文章计数",
        "无查询参数",
        "200 JSON items[{id,name,slug,color,article_count}]",
        "article_count 只统计 published，按 sort_order/id 稳定排序，空分类仍返回"
    ),
    endpoint!(
        "TAXONOMY-02",
        "分类标签",
        Get,
        "/api/v1/tags",
        "/api/v1/tags",
        Anonymous,
        OK,
        "读取标签及文章计数",
        "无查询参数",
        "200 JSON items[{id,name,article_count}]",
        "article_count 只统计 published；按使用量倒序后名称稳定排序"
    ),
    // 评论域：文章评论与说说回复共表，但请求必须且只能选择一个目标。
    endpoint!(
        "COMMENT-01",
        "评论",
        Get,
        "/api/v1/comments",
        "/api/v1/comments?article_slug=p1&page=1&per_page=10",
        Anonymous,
        OK,
        "读取已通过评论和楼中楼回复",
        "Query: article_slug 或 moment_id 二选一且必填，另有 page/per_page",
        "200 分页 items[{id,parent_id,author_name,author_site,is_owner,content,created_at}]",
        "features.comments=false 返回 403；仅 approved；父评论不存在或目标组合非法返回 422"
    ),
    endpoint!(
        "COMMENT-02",
        "评论",
        Post,
        "/api/v1/comments",
        "/api/v1/comments",
        Anonymous,
        CREATED,
        "提交游客评论或回复",
        "JSON: article_slug|moment_id 二选一、parent_id?、author_name/email/site?、content",
        "201 JSON id、status=pending、created_at",
        "按 IP+visitor 限流 3/分钟；校验长度与父子同目标；AI 预审只写判定，游客初始状态仍为 pending"
    ),
    // 说说公开域：HTTP 负责展示、点赞和评论；内容维护走下方后台接口。
    endpoint!(
        "MOMENT-01",
        "说说",
        Get,
        "/api/v1/moments",
        "/api/v1/moments?page=1&per_page=10&visitor_id=test-visitor",
        Anonymous,
        OK,
        "分页读取时间轴说说",
        "Query: page/per_page、可选 visitor_id",
        "200 分页 items[{id,content,images,like_count,reply_count,created_at,liked_by_me}]",
        "按 created_at 倒序；未传 visitor_id 时 liked_by_me 恒为 false；reply_count 只计 approved"
    ),
    endpoint!(
        "MOMENT-02",
        "说说",
        Post,
        "/api/v1/moments/{id}/like",
        "/api/v1/moments/1/like",
        Anonymous,
        OK,
        "切换当前访客点赞状态",
        "路径 id 为正整数；JSON: visitor_id",
        "200 JSON like_count、liked",
        "同一 visitor_id 对同一说说执行原子切换；限流 30/分钟；目标不存在返回 404"
    ),
    // 娱乐内容公开域：番剧由 Bilibili 同步，游戏由后台接口维护，前台均为只读。
    endpoint!(
        "MEDIA-01",
        "追番游戏",
        Get,
        "/api/v1/bangumi",
        "/api/v1/bangumi?page=1&per_page=10",
        Anonymous,
        OK,
        "读取 Bilibili 追番镜像",
        "Query: status=watching|finished、page/per_page",
        "200 分页 items，meta 包含 counts{watching,finished} 和 synced_at",
        "只读本地同步镜像，不在请求内访问 Bilibili；非法状态返回 422"
    ),
    endpoint!(
        "MEDIA-02",
        "追番游戏",
        Get,
        "/api/v1/games",
        "/api/v1/games?page=1&per_page=10",
        Anonymous,
        OK,
        "读取游戏列表",
        "Query: status=playing|finished、page/per_page",
        "200 分页 items，meta 包含 counts{playing,finished}",
        "按 sort_order/id 排序；非法状态返回 422；没有详情端点"
    ),
    // 友链公开域：公开提交只进入待审核状态，审核由下方后台接口完成。
    endpoint!(
        "FRIEND-01",
        "友链",
        Get,
        "/api/v1/friends",
        "/api/v1/friends?page=1&per_page=10",
        Anonymous,
        OK,
        "读取已通过友链",
        "Query: page/per_page",
        "200 分页 items[{name,url,avatar_url,description}]",
        "仅返回 approved，avatar_url 由当前 MinIO 素材版本派生；按 sort_order/created_at 排序；total 用于前台计数"
    ),
    endpoint!(
        "FRIEND-02",
        "友链",
        Post,
        "/api/v1/friends",
        "/api/v1/friends",
        Anonymous,
        CREATED,
        "提交友链申请",
        "JSON: name、url、avatar_url?、description?",
        "201 JSON id、status=pending",
        "按 IP 限流 2/小时；avatar_url 只是待审核来源，批准前须转存 MinIO 并建立 avatar_asset_id；重复 URL 返回 409；申请只能创建为 pending"
    ),
    // 站点域：聚合初始化、统计上报、搜索和 SEO 输出。
    endpoint!(
        "SITE-01",
        "站点",
        Get,
        "/api/v1/site",
        "/api/v1/site",
        Anonymous,
        OK,
        "读取全站初始化聚合信息",
        "无查询参数",
        "200 JSON basic、features、theme_rule、stats、about",
        "聚合只读配置和派生统计；uptime_days 以 Asia/Shanghai 日界计算；不得下发私密配置"
    ),
    endpoint!(
        "SITE-02",
        "站点",
        Post,
        "/api/v1/stats/visit",
        "/api/v1/stats/visit",
        Anonymous,
        NO_CONTENT,
        "上报一次页面访问",
        "JSON: visitor_id、path",
        "204 无响应体",
        "按上海日期 UPSERT PV；visitor_id 当日首次访问才增加 UV；限流 60/分钟"
    ),
    endpoint!(
        "SITE-03",
        "站点",
        Get,
        "/api/v1/search",
        "/api/v1/search?q=test&page=1&per_page=10",
        Anonymous,
        OK,
        "搜索已发布文章",
        "Query: q 非空且有长度上限、page/per_page",
        "200 分页 items[{slug,title,excerpt}]",
        "只搜 published；excerpt 为纯文本片段；限流 30/分钟；空关键词返回 422"
    ),
    endpoint!(
        "SITE-04",
        "站点",
        Get,
        "/api/v1/rss",
        "/api/v1/rss",
        Anonymous,
        OK,
        "输出 RSS 2.0 订阅源",
        "无查询参数",
        "200 application/rss+xml，最多包含最新 20 篇 published",
        "features.rss=false 返回 404；链接使用 PUBLIC_ORIGIN 生成绝对地址"
    ),
    endpoint!(
        "SITE-05",
        "站点",
        Get,
        "/api/v1/sitemap.xml",
        "/api/v1/sitemap.xml",
        Anonymous,
        OK,
        "输出站点地图",
        "无查询参数",
        "200 application/xml，包含静态页和全部 published 文章 URL",
        "不得包含草稿或后台 URL；lastmod 来自文章 updated_at；绝对地址使用 PUBLIC_ORIGIN"
    ),
    // 公开主题、音乐和看板娘域。
    endpoint!(
        "THEME-01",
        "主题媒体",
        Get,
        "/api/v1/themes",
        "/api/v1/themes",
        Anonymous,
        OK,
        "读取日夜双主题资源包",
        "无查询参数",
        "200 JSON day、night、rule，资源 URL 均可直接公开访问",
        "一次返回双主题，当前模式由前端本地决定；不得返回 MinIO 私钥或内部 object metadata"
    ),
    endpoint!(
        "THEME-02",
        "主题媒体",
        Get,
        "/api/v1/music",
        "/api/v1/music",
        Anonymous,
        OK,
        "读取 BGM 列表和播放配置",
        "无查询参数",
        "200 JSON items[{id,title,artist,file_url,duration_s}]、autoplay、default_volume",
        "features.music=false 返回 403；曲目按 sort_order/id 排序"
    ),
    endpoint!(
        "KANBAN-01",
        "看板娘",
        Get,
        "/api/v1/kanban/profile",
        "/api/v1/kanban/profile?theme=day",
        Anonymous,
        OK,
        "读取当前主题看板娘公开配置",
        "Query: theme=day|night 必填",
        "200 JSON persona_name、greeting_template、live2d_model_url、tts_enabled、tts_voice",
        "features.kanban=false 返回 403；按主题选择 persona、模型和日夜 TTS 音色；非法主题返回 422"
    ),
    endpoint!(
        "KANBAN-02",
        "看板娘",
        Post,
        "/api/v1/kanban/chat",
        "/api/v1/kanban/chat",
        Anonymous,
        OK,
        "代理一次看板娘对话",
        "JSON: session_id、message、article_slug?、theme",
        "200 JSON reply、motion?、egg?、fallback",
        "限流 6/分钟；最多回复 3 句；可选注入已发布文章上下文；LLM 故障时返回降级台词且 fallback=true"
    ),
    // 后台文章域：草稿创建、编辑、发布和批量状态变更。
    endpoint!(
        "ADMIN-ARTICLE-01",
        "后台文章",
        Get,
        "/api/v1/admin/articles",
        "/api/v1/admin/articles?page=1&per_page=10",
        AdminJwt,
        OK,
        "查询后台文章列表",
        "Query: page/per_page、status?、is_pinned?、sort?、search?",
        "200 分页返回草稿和已发布文章，包含 status、view_count、updated_at",
        "默认 updated_at 倒序；筛选枚举非法返回 422；后台可见全部状态"
    ),
    endpoint!(
        "ADMIN-ARTICLE-02",
        "后台文章",
        Post,
        "/api/v1/admin/articles",
        "/api/v1/admin/articles",
        AdminJwt,
        CREATED,
        "创建文章草稿",
        "JSON: 可选 title",
        "201 JSON id、slug、status=draft",
        "新文章只能先创建为 draft；slug 按 p{id} 稳定生成；空标题使用未命名草稿占位"
    ),
    endpoint!(
        "ADMIN-ARTICLE-03",
        "后台文章",
        Get,
        "/api/v1/admin/articles/{id}",
        "/api/v1/admin/articles/1",
        AdminJwt,
        OK,
        "读取文章编辑数据",
        "路径参数 id 为正整数",
        "200 返回文章全部可编辑字段及版本时间",
        "草稿和已发布均可读取；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-ARTICLE-04",
        "后台文章",
        Put,
        "/api/v1/admin/articles/{id}",
        "/api/v1/admin/articles/1",
        AdminJwt,
        OK,
        "保存或发布文章",
        "JSON: title、content_md、category_id、tag_ids、cover_asset_id?、content_asset_ids?、is_pinned、allow_comment、kanban_ref、status?",
        "200 返回 id、slug、status、word_count、read_minutes、updated_at",
        "全量覆盖且事务保存分类/标签/素材引用；素材类型必须兼容；status=published 时校验必填字段并首次写 published_at；计算字数和阅读时长"
    ),
    endpoint!(
        "ADMIN-ARTICLE-05",
        "后台文章",
        Delete,
        "/api/v1/admin/articles/{id}",
        "/api/v1/admin/articles/1",
        AdminJwt,
        NO_CONTENT,
        "删除单篇文章",
        "路径参数 id 为正整数",
        "204 无响应体",
        "事务删除文章及关联标签/评论；不存在返回 404；对象存储素材不在请求内物理删除"
    ),
    endpoint!(
        "ADMIN-ARTICLE-06",
        "后台文章",
        Post,
        "/api/v1/admin/articles/batch",
        "/api/v1/admin/articles/batch",
        AdminJwt,
        OK,
        "批量修改文章状态",
        "JSON: 非空 article_ids、action=publish|unpublish|delete|pin|unpin",
        "200 JSON affected、failed_ids",
        "同一事务处理可执行项；发布仍逐篇执行发布校验；空数组或未知 action 返回 422"
    ),
    // 后台评论域：AI 只提供预审建议，最终状态由管理员决定。
    endpoint!(
        "ADMIN-COMMENT-01",
        "后台评论",
        Get,
        "/api/v1/admin/comments",
        "/api/v1/admin/comments?status=pending&page=1&per_page=10",
        AdminJwt,
        OK,
        "分页读取审核队列",
        "Query: status=pending|approved|spam、page/per_page",
        "200 分页 items，包含 ai_verdict、ai_confidence、target",
        "文章评论和说说回复混排；按 created_at 倒序；非法状态返回 422"
    ),
    endpoint!(
        "ADMIN-COMMENT-02",
        "后台评论",
        Get,
        "/api/v1/admin/comments/counts",
        "/api/v1/admin/comments/counts",
        AdminJwt,
        OK,
        "读取评论状态计数",
        "无查询参数",
        "200 JSON pending、approved、spam、total",
        "四个计数来自同一一致性快照，total 等于三个状态之和"
    ),
    endpoint!(
        "ADMIN-COMMENT-03",
        "后台评论",
        Patch,
        "/api/v1/admin/comments/{id}",
        "/api/v1/admin/comments/1",
        AdminJwt,
        OK,
        "修改评论审核状态",
        "路径 id；JSON: status=approved|spam|pending",
        "200 JSON id、status、updated_at",
        "只允许三种状态互转；目标不存在返回 404；重复设置同状态保持幂等"
    ),
    endpoint!(
        "ADMIN-COMMENT-04",
        "后台评论",
        Post,
        "/api/v1/admin/comments/{id}/reply",
        "/api/v1/admin/comments/1/reply",
        AdminJwt,
        CREATED,
        "以博主身份回复评论",
        "路径 id；JSON: content",
        "201 JSON 回复对象，is_owner=true、status=approved",
        "回复与父评论目标一致并自动通过；父评论不存在返回 404；内容上限与游客评论相同"
    ),
    endpoint!(
        "ADMIN-COMMENT-05",
        "后台评论",
        Post,
        "/api/v1/admin/comments/approve-all",
        "/api/v1/admin/comments/approve-all",
        AdminJwt,
        OK,
        "批量通过待审评论",
        "JSON: 可选 ids；缺省表示全部 pending",
        "200 JSON affected",
        "只把 pending 改为 approved；显式 ids 含不存在项时忽略并返回实际 affected"
    ),
    // 后台分类与标签：公开列表仍只统计已发布文章，后台接口负责完整维护。
    endpoint!(
        "ADMIN-CATEGORY-01",
        "后台分类标签",
        Get,
        "/api/v1/admin/categories",
        "/api/v1/admin/categories",
        AdminJwt,
        OK,
        "读取全部分类及后台计数",
        "无查询参数",
        "200 JSON items[{id,name,slug,color,sort_order,published_count,draft_count}]",
        "返回空分类；按 sort_order/id 排序；计数覆盖全部文章状态"
    ),
    endpoint!(
        "ADMIN-CATEGORY-02",
        "后台分类标签",
        Post,
        "/api/v1/admin/categories",
        "/api/v1/admin/categories",
        AdminJwt,
        CREATED,
        "新建文章分类",
        "JSON: name、slug、color?、sort_order?",
        "201 返回完整分类",
        "name/slug 唯一；slug 规范化后不可为空；color 为空或 #RRGGBB"
    ),
    endpoint!(
        "ADMIN-CATEGORY-03",
        "后台分类标签",
        Patch,
        "/api/v1/admin/categories/{id}",
        "/api/v1/admin/categories/1",
        AdminJwt,
        OK,
        "修改分类名称、颜色或顺序",
        "路径 id；JSON 可包含 name、slug、color、sort_order",
        "200 返回更新后的完整分类",
        "至少提交一个字段；唯一键冲突返回 409；修改 slug 不改变文章关联"
    ),
    endpoint!(
        "ADMIN-CATEGORY-04",
        "后台分类标签",
        Delete,
        "/api/v1/admin/categories/{id}",
        "/api/v1/admin/categories/1",
        AdminJwt,
        NO_CONTENT,
        "删除未使用分类",
        "路径 id",
        "204 无响应体",
        "仍被任意文章引用时返回 409；不存在返回 404；重复删除返回 404"
    ),
    endpoint!(
        "ADMIN-TAG-01",
        "后台分类标签",
        Get,
        "/api/v1/admin/tags",
        "/api/v1/admin/tags",
        AdminJwt,
        OK,
        "读取全部标签及后台计数",
        "无查询参数",
        "200 JSON items[{id,name,published_count,draft_count}]",
        "返回未使用标签；按总使用量倒序、名称稳定排序"
    ),
    endpoint!(
        "ADMIN-TAG-02",
        "后台分类标签",
        Post,
        "/api/v1/admin/tags",
        "/api/v1/admin/tags",
        AdminJwt,
        CREATED,
        "新建文章标签",
        "JSON: name",
        "201 返回完整标签",
        "清理首尾空白后 name 唯一；重复返回 409"
    ),
    endpoint!(
        "ADMIN-TAG-03",
        "后台分类标签",
        Patch,
        "/api/v1/admin/tags/{id}",
        "/api/v1/admin/tags/1",
        AdminJwt,
        OK,
        "重命名文章标签",
        "路径 id；JSON: name",
        "200 返回更新后的完整标签",
        "空名称返回 422；重复名称返回 409；文章关联保持不变"
    ),
    endpoint!(
        "ADMIN-TAG-04",
        "后台分类标签",
        Delete,
        "/api/v1/admin/tags/{id}",
        "/api/v1/admin/tags/1",
        AdminJwt,
        NO_CONTENT,
        "删除标签",
        "路径 id",
        "204 无响应体",
        "事务删除标签及文章关联但不删除文章；不存在返回 404"
    ),
    // 后台说说：补齐原先只能通过 CLI 完成的发布和维护能力。
    endpoint!(
        "ADMIN-MOMENT-01",
        "后台说说",
        Get,
        "/api/v1/admin/moments",
        "/api/v1/admin/moments?page=1&per_page=10",
        AdminJwt,
        OK,
        "分页读取全部说说",
        "Query: page/per_page、search?",
        "200 分页 items，包含 images、like_count、reply_count、created_at、updated_at",
        "按 created_at 倒序；回复计数包含各审核状态分项"
    ),
    endpoint!(
        "ADMIN-MOMENT-02",
        "后台说说",
        Post,
        "/api/v1/admin/moments",
        "/api/v1/admin/moments",
        AdminJwt,
        CREATED,
        "发布说说",
        "JSON: content、asset_ids?、created_at?",
        "201 返回完整说说",
        "asset_ids 必须指向图片素材；按提交顺序建立引用；空内容且无图片返回 422"
    ),
    endpoint!(
        "ADMIN-MOMENT-03",
        "后台说说",
        Put,
        "/api/v1/admin/moments/{id}",
        "/api/v1/admin/moments/1",
        AdminJwt,
        OK,
        "编辑说说",
        "路径 id；JSON: content、asset_ids、created_at?",
        "200 返回更新后的完整说说",
        "事务替换图片引用；不重置点赞和回复；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-MOMENT-04",
        "后台说说",
        Delete,
        "/api/v1/admin/moments/{id}",
        "/api/v1/admin/moments/1",
        AdminJwt,
        NO_CONTENT,
        "删除说说",
        "路径 id",
        "204 无响应体",
        "事务删除点赞、回复和素材引用；素材本体保留；不存在返回 404"
    ),
    // 后台游戏：补齐游戏条目的网页维护与排序能力。
    endpoint!(
        "ADMIN-GAME-01",
        "后台游戏",
        Get,
        "/api/v1/admin/games",
        "/api/v1/admin/games?page=1&per_page=10",
        AdminJwt,
        OK,
        "分页读取全部游戏",
        "Query: page/per_page、status?、search?",
        "200 分页 items，包含 cover_asset、status、play_hours、comment、sort_order",
        "后台可见 playing/finished/shelved 全状态；按 sort_order/id 排序"
    ),
    endpoint!(
        "ADMIN-GAME-02",
        "后台游戏",
        Post,
        "/api/v1/admin/games",
        "/api/v1/admin/games",
        AdminJwt,
        CREATED,
        "新增游戏",
        "JSON: title、cover_asset_id?、status、play_hours、comment?",
        "201 返回完整游戏",
        "封面必须是图片素材；追加到排序末尾；时长不得为负"
    ),
    endpoint!(
        "ADMIN-GAME-03",
        "后台游戏",
        Put,
        "/api/v1/admin/games/{id}",
        "/api/v1/admin/games/1",
        AdminJwt,
        OK,
        "编辑游戏与进度",
        "路径 id；JSON 与创建字段同形",
        "200 返回更新后的完整游戏",
        "全量覆盖可编辑字段；封面引用原子替换；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-GAME-04",
        "后台游戏",
        Delete,
        "/api/v1/admin/games/{id}",
        "/api/v1/admin/games/1",
        AdminJwt,
        NO_CONTENT,
        "删除游戏",
        "路径 id",
        "204 无响应体",
        "删除记录和素材引用并压缩排序；素材本体保留；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-GAME-05",
        "后台游戏",
        Put,
        "/api/v1/admin/games/order",
        "/api/v1/admin/games/order",
        AdminJwt,
        OK,
        "批量调整游戏顺序",
        "JSON: order 为全部游戏 id 的无重复数组",
        "200 JSON items 为新顺序",
        "完整覆盖当前集合并事务更新连续 sort_order；缺失或重复 id 返回 422"
    ),
    // 后台友链：公开申请进入 pending，后台完成审核、编辑、排序和移除。
    endpoint!(
        "ADMIN-FRIEND-01",
        "后台友链",
        Get,
        "/api/v1/admin/friends",
        "/api/v1/admin/friends?status=pending&page=1&per_page=10",
        AdminJwt,
        OK,
        "分页读取全部友链申请",
        "Query: page/per_page、status=pending|approved|rejected?、search?",
        "200 分页 items，包含全部申请字段、status、sort_order、created_at、updated_at",
        "后台可见全部状态；按 status、sort_order、created_at 稳定排序"
    ),
    endpoint!(
        "ADMIN-FRIEND-02",
        "后台友链",
        Patch,
        "/api/v1/admin/friends/{id}",
        "/api/v1/admin/friends/1",
        AdminJwt,
        OK,
        "审核或编辑友链",
        "路径 id；JSON 可包含 name、url、avatar_url、avatar_asset_id、description、status",
        "200 返回更新后的完整友链",
        "至少一个字段；已通过友链必须引用 active 图片素材，公开 avatar_url 由 MinIO 当前版本派生；外链头像仅作为待审核来源；URL 冲突返回 409"
    ),
    endpoint!(
        "ADMIN-FRIEND-03",
        "后台友链",
        Delete,
        "/api/v1/admin/friends/{id}",
        "/api/v1/admin/friends/1",
        AdminJwt,
        NO_CONTENT,
        "移除友链或申请记录",
        "路径 id",
        "204 无响应体",
        "删除任意状态记录并压缩已通过列表顺序；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-FRIEND-04",
        "后台友链",
        Put,
        "/api/v1/admin/friends/order",
        "/api/v1/admin/friends/order",
        AdminJwt,
        OK,
        "调整已通过友链顺序",
        "JSON: order 为全部 approved 友链 id 的无重复数组",
        "200 JSON items 为新顺序",
        "只排序 approved；完整覆盖当前集合；失败整体回滚"
    ),
    // 素材库：二进制全部存 MinIO，数据库只管理逻辑素材、版本和引用。
    endpoint!(
        "ASSET-01",
        "素材库",
        Get,
        "/api/v1/admin/assets",
        "/api/v1/admin/assets?page=1&per_page=20",
        AdminJwt,
        OK,
        "分页搜索素材库",
        "Query: page/per_page、media_type=image|audio|video|live2d|font|other?、search?、sort=uploaded_at|name|size?、order=asc|desc?、usable_for?",
        "200 分页 items；meta 包含各类型计数、total_size_bytes、quota_bytes",
        "只返回 active 素材；usable_for 按目标接受类型过滤；列表返回当前版本和 reference_count"
    ),
    asset_upload_endpoint!(
        "ASSET-02",
        "素材库",
        Post,
        "/api/v1/admin/assets",
        "/api/v1/admin/assets",
        AdminJwt,
        CREATED,
        "上传新素材到 MinIO",
        "multipart: file、name?、media_type?；单文件最大 200MB",
        "201 返回素材及 v1 当前版本、公开或受控访问 URL",
        "按内容嗅探而非扩展名确定类型；对象先写 MinIO 再原子登记，失败执行补偿清理"
    ),
    endpoint!(
        "ASSET-03",
        "素材库",
        Get,
        "/api/v1/admin/assets/{id}",
        "/api/v1/admin/assets/1",
        AdminJwt,
        OK,
        "读取素材详情、版本和引用",
        "路径 id",
        "200 JSON asset、current_version、versions[]、references[]、preview",
        "references 包含可读位置和后台跳转路径；不得返回 MinIO 密钥；不存在返回 404"
    ),
    endpoint!(
        "ASSET-04",
        "素材库",
        Patch,
        "/api/v1/admin/assets/{id}",
        "/api/v1/admin/assets/1",
        AdminJwt,
        OK,
        "重命名素材",
        "路径 id；JSON: name",
        "200 返回更新后的素材",
        "只改显示名，不改 MinIO object_key 或已有引用；空名称返回 422"
    ),
    asset_upload_endpoint!(
        "ASSET-05",
        "素材库",
        Post,
        "/api/v1/admin/assets/{id}/versions",
        "/api/v1/admin/assets/1/versions",
        AdminJwt,
        CREATED,
        "替换素材文件并创建新版本",
        "路径 id；multipart: file；单文件最大 200MB",
        "201 返回新版本和更新后的素材",
        "新文件媒体类型必须兼容原素材；旧版本保留；切换 current_version 后所有逻辑引用自动生效"
    ),
    endpoint!(
        "ASSET-06",
        "素材库",
        Post,
        "/api/v1/admin/assets/{id}/versions/{version}/restore",
        "/api/v1/admin/assets/1/versions/1/restore",
        AdminJwt,
        OK,
        "回滚到历史素材版本",
        "路径 id、version 为正整数版本号",
        "200 返回素材和新的当前版本",
        "历史版本必须属于该素材；只切换 current_version，不复制 MinIO 对象；重复回滚幂等"
    ),
    endpoint!(
        "ASSET-07",
        "素材库",
        Delete,
        "/api/v1/admin/assets/{id}",
        "/api/v1/admin/assets/1",
        AdminJwt,
        NO_CONTENT,
        "删除未被引用的素材及全部版本",
        "路径 id",
        "204 无响应体",
        "reference_count>0 返回 409；先标记删除再清理 MinIO 全部版本；部分失败可重试且不可留下可见脏数据"
    ),
    endpoint!(
        "ASSET-08",
        "素材库",
        Post,
        "/api/v1/admin/assets/batch-delete",
        "/api/v1/admin/assets/batch-delete",
        AdminJwt,
        OK,
        "批量删除可删除素材",
        "JSON: 非空 asset_ids，最多 100 个",
        "200 JSON deleted_ids、blocked[{id,reference_count}]、missing_ids",
        "有引用项只进入 blocked，不影响其余可删项；每个素材删除规则与 ASSET-07 相同"
    ),
    endpoint!(
        "ASSET-09",
        "素材库",
        Post,
        "/api/v1/admin/assets/batch-download",
        "/api/v1/admin/assets/batch-download",
        AdminJwt,
        OK,
        "打包下载素材当前版本",
        "JSON: 非空 asset_ids，最多 100 个",
        "200 application/zip 流式响应，Content-Disposition 使用安全文件名",
        "只读取各素材当前版本并从 MinIO 流式打包；限制总展开大小；临时文件必须清理"
    ),
    // 后台主题与媒体域：配置端点采用全量覆盖或明确的单资源更新。
    endpoint!(
        "ADMIN-THEME-01",
        "后台主题媒体",
        Get,
        "/api/v1/admin/themes",
        "/api/v1/admin/themes",
        AdminJwt,
        OK,
        "读取日夜主题编辑配置",
        "无查询参数",
        "200 JSON day、night、rule，含 cover_asset/voice_asset 及服务端派生 URL/文件信息",
        "quote_zh 等无页面控件字段也必须下发，前端全量保存时原样回传"
    ),
    endpoint!(
        "ADMIN-THEME-02",
        "后台主题媒体",
        Put,
        "/api/v1/admin/themes",
        "/api/v1/admin/themes",
        AdminJwt,
        OK,
        "全量保存日夜主题",
        "JSON 与 GET 可编辑字段同形；使用 cover_asset_id/voice_asset_id，派生 URL/文件信息提交时忽略",
        "200 返回规范化后的完整配置",
        "在单事务中更新 day/night、素材引用与 rule；素材必须存在、处于 active 且类型匹配"
    ),
    endpoint!(
        "ADMIN-THEME-03",
        "后台主题媒体",
        Post,
        "/api/v1/admin/themes/reset",
        "/api/v1/admin/themes/reset",
        AdminJwt,
        OK,
        "重置主题默认值",
        "无请求体",
        "200 返回重置后的完整主题配置",
        "重置数据库配置但不删除已上传素材；操作应可重复执行"
    ),
    endpoint!(
        "ADMIN-MUSIC-01",
        "后台主题媒体",
        Get,
        "/api/v1/admin/music",
        "/api/v1/admin/music",
        AdminJwt,
        OK,
        "读取全部 BGM 和播放配置",
        "无查询参数且免分页",
        "200 JSON items、autoplay、default_volume",
        "按 sort_order/id 返回全部曲目"
    ),
    endpoint!(
        "ADMIN-MUSIC-02",
        "后台主题媒体",
        Post,
        "/api/v1/admin/music",
        "/api/v1/admin/music",
        AdminJwt,
        CREATED,
        "新增 BGM 曲目",
        "JSON: title、artist、asset_id、duration_s?",
        "201 返回完整曲目",
        "asset_id 必须是 active 音频素材；时长优先取素材元数据；追加到排序末尾"
    ),
    endpoint!(
        "ADMIN-MUSIC-03",
        "后台主题媒体",
        Put,
        "/api/v1/admin/music/order",
        "/api/v1/admin/music/order",
        AdminJwt,
        OK,
        "批量调整 BGM 顺序",
        "JSON: order 为无重复曲目 id 数组",
        "200 JSON items 为新顺序",
        "order 必须完整覆盖当前曲目集合；事务更新连续 sort_order，缺失/重复 id 返回 422"
    ),
    endpoint!(
        "ADMIN-MUSIC-04",
        "后台主题媒体",
        Delete,
        "/api/v1/admin/music/{id}",
        "/api/v1/admin/music/1",
        AdminJwt,
        NO_CONTENT,
        "删除 BGM 曲目",
        "路径参数 id 为正整数",
        "204 无响应体",
        "删除曲目记录并压缩排序；素材对象仅解除引用，不在此端点物理删除；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-MUSIC-05",
        "后台主题媒体",
        Put,
        "/api/v1/admin/music/settings",
        "/api/v1/admin/music/settings",
        AdminJwt,
        OK,
        "保存 BGM 播放设置",
        "JSON: autoplay、default_volume=0..1",
        "200 JSON autoplay、default_volume",
        "音量越界返回 422；写入 site_settings.music 并使公开缓存失效"
    ),
    endpoint!(
        "ADMIN-VOICE-01",
        "后台主题媒体",
        Get,
        "/api/v1/admin/voices",
        "/api/v1/admin/voices",
        AdminJwt,
        OK,
        "读取日夜开屏语音和 TTS 设置",
        "无查询参数",
        "200 JSON day、night、tts{enabled,day_voice,night_voice}",
        "语音 transcript/credit 与 theme_configs 共用字段，必须保持跨页面一致"
    ),
    endpoint!(
        "ADMIN-VOICE-02",
        "后台主题媒体",
        Put,
        "/api/v1/admin/voices/{mode}",
        "/api/v1/admin/voices/day",
        AdminJwt,
        OK,
        "替换日间或夜间开屏语音",
        "路径 mode=day|night；JSON: asset_id、transcript?、credit?",
        "200 返回该 mode 的规范化语音配置",
        "asset_id 必须是 active 音频素材；非法 mode 返回 404；同时更新对应 theme_config 引用"
    ),
    endpoint!(
        "ADMIN-VOICE-03",
        "后台主题媒体",
        Get,
        "/api/v1/admin/voices/options",
        "/api/v1/admin/voices/options",
        AdminJwt,
        OK,
        "读取浏览器 TTS 音色枚举",
        "无查询参数且免分页",
        "200 JSON items[{id,name,theme}]",
        "枚举来自后端常量；theme 仅用于分组提示，不限制日夜选择"
    ),
    endpoint!(
        "ADMIN-VOICE-04",
        "后台主题媒体",
        Put,
        "/api/v1/admin/voices/tts",
        "/api/v1/admin/voices/tts",
        AdminJwt,
        OK,
        "保存 TTS 开关与日夜音色",
        "JSON: enabled、day_voice、night_voice",
        "200 返回规范化后的 tts 配置",
        "两个 voice id 均须存在于枚举，允许相同；更新后看板娘 profile 立即按主题下发"
    ),
    // 后台看板娘域：配置、沙盒测试、状态和 Live2D 资源。
    endpoint!(
        "ADMIN-KANBAN-01",
        "后台看板娘",
        Get,
        "/api/v1/admin/kanban/config",
        "/api/v1/admin/kanban/config",
        AdminJwt,
        OK,
        "读取看板娘与 LLM 全部配置",
        "无查询参数",
        "200 JSON live2d、sync_theme_persona、LLM 参数、prompts、personas、triggers",
        "敏感 API 密钥不通过本端点下发；返回完整可回传配置"
    ),
    endpoint!(
        "ADMIN-KANBAN-02",
        "后台看板娘",
        Put,
        "/api/v1/admin/kanban/config",
        "/api/v1/admin/kanban/config",
        AdminJwt,
        OK,
        "全量保存看板娘配置",
        "JSON 与 GET 可编辑字段同形",
        "200 返回规范化后的完整配置",
        "校验模型、temperature、max_tokens、Live2D 引用和 trigger 唯一性后原子覆盖"
    ),
    endpoint!(
        "ADMIN-KANBAN-03",
        "后台看板娘",
        Get,
        "/api/v1/admin/kanban/models",
        "/api/v1/admin/kanban/models",
        AdminJwt,
        OK,
        "读取允许使用的 LLM 模型",
        "无查询参数且免分页",
        "200 JSON items[string]",
        "只返回后端允许列表，不把任意客户端模型名直接透传到供应商"
    ),
    endpoint!(
        "ADMIN-KANBAN-04",
        "后台看板娘",
        Post,
        "/api/v1/admin/kanban/test",
        "/api/v1/admin/kanban/test",
        AdminJwt,
        OK,
        "使用草稿配置进行沙盒对话",
        "JSON: message、persona、draft_config",
        "200 JSON reply、triggered_egg?、motion?",
        "不得落库 draft_config 或正式聊天统计；仍执行参数校验和供应商超时保护"
    ),
    endpoint!(
        "ADMIN-KANBAN-05",
        "后台看板娘",
        Get,
        "/api/v1/admin/kanban/status",
        "/api/v1/admin/kanban/status",
        AdminJwt,
        OK,
        "读取 LLM 在线状态和耗时",
        "无查询参数",
        "200 JSON online、avg_ms",
        "online 来自轻量健康探测/近期结果；avg_ms 由 assistant 日志聚合，无样本时为 null"
    ),
    endpoint!(
        "ADMIN-LIVE2D-01",
        "后台看板娘",
        Get,
        "/api/v1/admin/live2d/models",
        "/api/v1/admin/live2d/models",
        AdminJwt,
        OK,
        "读取已上传 Live2D 模型",
        "无查询参数且免分页",
        "200 JSON items[{id,name,asset_id,model_url,thumbnail_url}]",
        "只返回引用 active Live2D 素材且入口 model3.json 存在的模型，按 created_at 倒序"
    ),
    endpoint!(
        "ADMIN-LIVE2D-02",
        "后台看板娘",
        Post,
        "/api/v1/admin/live2d/models",
        "/api/v1/admin/live2d/models",
        AdminJwt,
        CREATED,
        "从素材库登记 Live2D 模型",
        "JSON: asset_id、name?；asset 必须是已上传的 live2d 素材",
        "201 JSON id、name、asset_id、model_url、thumbnail_url",
        "素材上传阶段已完成 Zip Slip/压缩炸弹检查；必须恰有可确定入口的 .model3.json；重复登记返回 409"
    ),
    // 后台统计与站点运维域。
    endpoint!(
        "ADMIN-STATS-01",
        "后台统计运维",
        Get,
        "/api/v1/admin/stats/overview",
        "/api/v1/admin/stats/overview",
        AdminJwt,
        OK,
        "读取仪表盘概览",
        "无查询参数",
        "200 JSON 今日访客及差值、文章/草稿/评论/对话/LLM/运行天数",
        "所有日维度数据按 Asia/Shanghai；同一响应尽量使用一致性快照"
    ),
    endpoint!(
        "ADMIN-STATS-02",
        "后台统计运维",
        Get,
        "/api/v1/admin/stats/pv-uv",
        "/api/v1/admin/stats/pv-uv?days=14",
        AdminJwt,
        OK,
        "读取近 N 天 PV/UV",
        "Query: days 默认 14，范围 1..90",
        "200 JSON items[{date,pv,uv}]，缺失日期补零",
        "日期升序且连续；越界 days 返回 422"
    ),
    endpoint!(
        "ADMIN-SITE-01",
        "后台统计运维",
        Get,
        "/api/v1/admin/site/settings",
        "/api/v1/admin/site/settings",
        AdminJwt,
        OK,
        "读取全部站点设置",
        "无查询参数",
        "200 返回按 basic/features/theme/about/music/bangumi_sync 等分组的完整 JSON",
        "只下发可编辑配置，数据库连接、MinIO/LLM 密钥等环境机密必须排除"
    ),
    endpoint!(
        "ADMIN-SITE-02",
        "后台统计运维",
        Put,
        "/api/v1/admin/site/settings",
        "/api/v1/admin/site/settings",
        AdminJwt,
        OK,
        "保存站点基本信息",
        "JSON: basic{name,tagline,icp}，domain 为只读不可修改",
        "200 JSON basic，包含服务端保留的 domain/founded_at",
        "全量覆盖 basic 可编辑字段；清理空白并校验长度；更新后使公开 site 缓存失效"
    ),
    endpoint!(
        "ADMIN-SITE-03",
        "后台统计运维",
        Patch,
        "/api/v1/admin/site/settings",
        "/api/v1/admin/site/settings",
        AdminJwt,
        OK,
        "乐观更新单个站点配置",
        "JSON 每次只能包含一个允许的开关或配置叶字段",
        "200 返回被更新字段及 updated_at",
        "拒绝多字段和未知路径；uid 变化后提交异步全量追番同步；相关公开缓存立即失效"
    ),
    endpoint!(
        "ADMIN-BANGUMI-01",
        "后台统计运维",
        Post,
        "/api/v1/admin/bangumi/sync",
        "/api/v1/admin/bangumi/sync",
        AdminJwt,
        ACCEPTED,
        "立即触发 Bilibili 追番同步",
        "无请求体",
        "202 JSON job_id、status=queued",
        "未配置 uid 返回 422；已有同步任务运行时返回 409；后台任务 UPSERT 镜像并转存封面"
    ),
    endpoint!(
        "ADMIN-BACKUP-01",
        "后台统计运维",
        Get,
        "/api/v1/admin/site/backup",
        "/api/v1/admin/site/backup",
        AdminJwt,
        OK,
        "读取备份计划和历史",
        "无查询参数",
        "200 JSON schedule、last_backup_at、last_status、items",
        "备份对象位于私有桶；响应只返回记录和状态，不生成公开下载 URL"
    ),
    endpoint!(
        "ADMIN-BACKUP-02",
        "后台统计运维",
        Post,
        "/api/v1/admin/site/backup",
        "/api/v1/admin/site/backup",
        AdminJwt,
        ACCEPTED,
        "立即创建数据库备份",
        "无请求体",
        "202 JSON job_id、status=queued",
        "已有备份运行时返回 409；后台执行 pg_dump 压缩并写私有桶，成功/失败都记录 backups"
    ),
    endpoint!(
        "ADMIN-CACHE-01",
        "后台统计运维",
        Post,
        "/api/v1/admin/site/cache/clear",
        "/api/v1/admin/site/cache/clear",
        AdminJwt,
        NO_CONTENT,
        "清空后端只读缓存",
        "无请求体",
        "204 无响应体",
        "只清 site/themes/music 等进程内缓存，不删除数据库或 MinIO 对象；重复执行幂等"
    ),
];

/// 构建全部尚未实现的业务路由。
///
/// 后续实现某个域时，应从此处移除对应占位注册，并在该域模块用完全相同的方法和路径
/// 注册真实处理器；契约测试会阻止漏路由、方法漂移或重复注册。
pub fn router() -> Router<AppState> {
    ENDPOINT_CONTRACTS
        .iter()
        .filter(|contract| !crate::auth::implements(contract.method, contract.path))
        .fold(Router::new(), |router, contract| {
            router.route(
                contract.path,
                on(contract.method.filter(), not_implemented)
                    .layer(DefaultBodyLimit::max(contract.max_body_bytes)),
            )
        })
}

/// 契约已登记但业务尚未编写时的统一响应。
async fn not_implemented(method: Method, OriginalUri(uri): OriginalUri) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorEnvelope {
            error: ErrorBody {
                code: "not_implemented",
                message: format!(
                    "{} {} is defined by the API contract but has no business implementation yet",
                    method,
                    uri.path()
                ),
            },
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::{Authentication, ENDPOINT_CONTRACTS};
    use crate::{build_app, config::Config, state::AppState};

    /// 构造不建立真实数据库连接的应用，供路由契约测试使用。
    ///
    /// 当前测试只访问占位处理器，因此 lazy pool 不会产生网络 I/O；业务实现开始访问
    /// PostgreSQL 后，应把对应验收用例迁到具备隔离测试库的集成测试层。
    fn test_app() -> axum::Router {
        let config = Config {
            environment: "test".to_owned(),
            host: "127.0.0.1".parse().expect("valid test host"),
            port: 3000,
            database_url: "postgres://test:test@localhost/test".to_owned(),
            db_max_connections: 1,
            db_min_connections: 0,
            run_migrations: false,
            minio_endpoint: "http://localhost:9000".to_owned(),
            minio_access_key: "test".to_owned(),
            minio_secret_key: "test".to_owned(),
            minio_public_bucket: "blog-public".to_owned(),
            minio_private_bucket: "blog-private".to_owned(),
            admin_username: "test".to_owned(),
            admin_initial_password: Some("test".to_owned()),
            auth_jwt_secret: "test-secret-at-least-32-bytes-long".to_owned(),
            public_origin: "http://localhost".to_owned(),
            cors_allowed_origins: vec!["http://localhost:5173".to_owned()],
            request_timeout_secs: 5,
        };
        let pool = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .expect("valid lazy PostgreSQL URL");
        let state = AppState::new(pool, &config).expect("valid test state");

        build_app(state, &config).expect("contract app should build")
    }

    /// TDD 契约层：固定端点总数、唯一性和每项说明的完整性。
    #[test]
    fn catalog_contains_106_complete_unique_contracts() {
        assert_eq!(ENDPOINT_CONTRACTS.len(), 106);

        let mut ids = HashSet::new();
        let mut method_paths = HashSet::new();
        for contract in ENDPOINT_CONTRACTS {
            assert!(
                ids.insert(contract.id),
                "duplicate contract id {}",
                contract.id
            );
            assert!(
                method_paths.insert((contract.method, contract.path)),
                "duplicate endpoint {} {}",
                contract.method.as_str(),
                contract.path
            );
            assert!(
                contract.path.starts_with("/api/v1/"),
                "{} has invalid prefix",
                contract.id
            );
            assert!(
                !contract.example_path.contains('{'),
                "{} has unresolved test path",
                contract.id
            );
            assert!(
                contract.success_status.is_success(),
                "{} lacks a success status",
                contract.id
            );
            assert!(
                !contract.domain.trim().is_empty(),
                "{} lacks domain comments",
                contract.id
            );
            assert!(
                !contract.summary.trim().is_empty(),
                "{} lacks summary comments",
                contract.id
            );
            assert!(
                !contract.request.trim().is_empty(),
                "{} lacks request comments",
                contract.id
            );
            assert!(
                !contract.response.trim().is_empty(),
                "{} lacks response comments",
                contract.id
            );
            assert!(
                !contract.business_rule.trim().is_empty(),
                "{} lacks business-rule comments",
                contract.id
            );
            assert!(
                contract.max_body_bytes > 0
                    && contract.max_body_bytes <= super::ASSET_UPLOAD_LIMIT_BYTES,
                "{} has an invalid request-body limit",
                contract.id
            );
        }

        let admin_count = ENDPOINT_CONTRACTS
            .iter()
            .filter(|contract| contract.path.starts_with("/api/v1/admin/"))
            .count();
        assert_eq!(admin_count, 84);
        assert_eq!(ENDPOINT_CONTRACTS.len() - admin_count, 22);

        let large_body_endpoints = ENDPOINT_CONTRACTS
            .iter()
            .filter(|contract| contract.max_body_bytes == super::ASSET_UPLOAD_LIMIT_BYTES)
            .map(|contract| (contract.method.as_str(), contract.path))
            .collect::<HashSet<_>>();
        assert_eq!(
            large_body_endpoints,
            HashSet::from([
                ("POST", "/api/v1/admin/assets"),
                ("POST", "/api/v1/admin/assets/{id}/versions"),
            ])
        );
    }

    /// 固定整个目录的身份快照，避免“数量仍是 106，但某个方法或路径被悄悄替换”。
    #[test]
    fn catalog_identity_matches_the_reviewed_snapshot() {
        let mut fingerprint = 0xcbf29ce484222325_u64;

        for contract in ENDPOINT_CONTRACTS {
            let authentication = match contract.authentication {
                Authentication::Anonymous => "anonymous",
                Authentication::RefreshCookie => "refresh_cookie",
                Authentication::AdminJwt => "admin_jwt",
            };
            for part in [
                contract.id.to_owned(),
                contract.method.as_str().to_owned(),
                contract.path.to_owned(),
                contract.example_path.to_owned(),
                authentication.to_owned(),
                contract.success_status.as_u16().to_string(),
                contract.max_body_bytes.to_string(),
            ] {
                for byte in part.as_bytes() {
                    fingerprint ^= u64::from(*byte);
                    fingerprint = fingerprint.wrapping_mul(0x100000001b3);
                }
                fingerprint ^= 0xff;
                fingerprint = fingerprint.wrapping_mul(0x100000001b3);
            }
        }

        assert_eq!(
            fingerprint, 15_744_337_252_995_643_575,
            "update only after reviewing the full catalog"
        );
    }

    /// 代表性 URL 必须真的匹配其路径模板，不能只检查花括号已经被替换。
    #[test]
    fn every_example_path_matches_its_template() {
        for contract in ENDPOINT_CONTRACTS {
            let example_path = contract.example_path.split('?').next().unwrap();
            let template_segments = contract.path.split('/').collect::<Vec<_>>();
            let example_segments = example_path.split('/').collect::<Vec<_>>();

            assert_eq!(
                template_segments.len(),
                example_segments.len(),
                "{} example has a different segment count",
                contract.id
            );
            for (template, example) in template_segments.iter().zip(example_segments) {
                if template.starts_with('{') && template.ends_with('}') {
                    assert!(
                        !example.is_empty(),
                        "{} has an empty path parameter",
                        contract.id
                    );
                } else {
                    assert_eq!(
                        *template, example,
                        "{} example does not match its route template",
                        contract.id
                    );
                }
            }
        }
    }

    /// TDD 安全层：公开端点和后台会话要求必须显式声明，防止新增后台路由漏鉴权。
    #[test]
    fn authentication_contract_matches_the_public_allowlist() {
        let anonymous_admin_allowlist = HashSet::from([
            "/api/v1/admin/auth/login",
            "/api/v1/admin/auth/passkey/options",
            "/api/v1/admin/auth/passkey/verify",
            "/api/v1/admin/auth/forgot-password",
        ]);

        for contract in ENDPOINT_CONTRACTS {
            if contract.path == "/api/v1/admin/auth/refresh" {
                assert_eq!(contract.authentication, Authentication::RefreshCookie);
            } else if anonymous_admin_allowlist.contains(contract.path) {
                assert_eq!(contract.authentication, Authentication::Anonymous);
            } else if contract.path.starts_with("/api/v1/admin/") {
                assert_eq!(contract.authentication, Authentication::AdminJwt);
            } else {
                assert_eq!(contract.authentication, Authentication::Anonymous);
            }
        }
    }

    /// TDD 路由层：逐个执行 102 个代表请求，确认路径和方法已登记。
    ///
    /// 测试刻意不强制所有端点永远返回 501：某个占位处理器被真实实现替换后，空请求
    /// 可能得到 400/401/422 或成功响应，只要不是 404/405 就证明方法与路径仍然存在。
    /// 尚处于占位状态的端点则必须继续满足统一错误信封。
    #[tokio::test]
    async fn every_contract_has_an_executable_placeholder_route() {
        let app = test_app();

        for contract in ENDPOINT_CONTRACTS {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(contract.method.http_method())
                        .uri(contract.example_path)
                        .body(Body::empty())
                        .expect("valid contract request"),
                )
                .await
                .expect("placeholder should respond");

            assert_ne!(
                response.status(),
                StatusCode::NOT_FOUND,
                "{} {} must keep its declared path",
                contract.method.as_str(),
                contract.example_path
            );
            assert_ne!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{} {} must keep its declared HTTP method",
                contract.method.as_str(),
                contract.example_path
            );
            assert!(response.headers().contains_key("x-request-id"));

            if response.status() == StatusCode::NOT_IMPLEMENTED {
                assert_eq!(
                    response
                        .headers()
                        .get("content-type")
                        .and_then(|value| value.to_str().ok()),
                    Some("application/json"),
                    "{} must use the standard JSON error envelope",
                    contract.id
                );
                let body = response
                    .into_body()
                    .collect()
                    .await
                    .expect("read placeholder body")
                    .to_bytes();
                let json: Value = serde_json::from_slice(&body).expect("placeholder JSON");
                assert_eq!(
                    json["error"]["code"], "not_implemented",
                    "{} error code",
                    contract.id
                );
            }
        }
    }

    /// 每个已登记路径的不受支持方法都必须返回 405，而不是误落入 JSON 404 fallback。
    #[tokio::test]
    async fn registered_paths_reject_unsupported_methods() {
        let app = test_app();
        let mut paths = HashMap::<&str, (&str, HashSet<axum::http::Method>)>::new();
        for contract in ENDPOINT_CONTRACTS {
            let entry = paths
                .entry(contract.path)
                .or_insert_with(|| (contract.example_path, HashSet::new()));
            entry.1.insert(contract.method.http_method());
        }

        let candidates = [
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
        ];
        for (template, (example, supported)) in paths {
            let method = candidates
                .iter()
                .find(|candidate| !supported.contains(*candidate))
                .expect("each route has at least one unsupported method");
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.clone())
                        .uri(example)
                        .body(Body::empty())
                        .expect("valid request"),
                )
                .await
                .expect("router response");

            assert_eq!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {template} must be rejected by the registered path"
            );
        }
    }
}
