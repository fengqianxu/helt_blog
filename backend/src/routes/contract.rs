//! 后端 HTTP 契约目录与未实现路由骨架。
//!
//! 本模块是 TDD 的第一层防线：先固定每个端点的方法、路径、认证方式、
//! 成功状态码、请求/响应形状和核心业务规则，再逐个把占位处理器替换为真实实现。
//! 在业务处理器完成前，已登记端点统一返回结构化 `501 Not Implemented`；这能让前端
//! 区分“契约已存在但尚未实现”和“路径写错导致 404”。

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, OriginalUri, State},
    http::{HeaderMap, Method, StatusCode},
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

/// multipart 请求还需要容纳边界、文件名和表单字段；网关使用相同的 202 MB 上限。
pub const ASSET_MULTIPART_BODY_LIMIT_BYTES: usize =
    ASSET_UPLOAD_LIMIT_BYTES + DEFAULT_REQUEST_BODY_LIMIT_BYTES;

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
    /// 路由允许读取的最大请求体；普通接口 2 MB，素材上传请求 202 MB。
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
            ASSET_MULTIPART_BODY_LIMIT_BYTES
        )
    };
}

/// v1 的全部 99 个业务端点。
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
        "/api/v1/admin/auth/passkey/login/options",
        "/api/v1/admin/auth/passkey/login/options",
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
        "/api/v1/admin/auth/passkey/login/verify",
        "/api/v1/admin/auth/passkey/login/verify",
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
        "200 JSON username、email、avatar_url、bilibili_uid",
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
        "JSON: current_password、new_password、revoke_other_sessions?；新密码长度为 12–128 个字符",
        "204 无响应体并清除当前认证 cookie",
        "必须复核当前密码；当前 refresh token 始终吊销；revoke_other_sessions=true 时额外吊销其他设备 refresh token"
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
        "JSON: email、bilibili_uid、steam_web_api_key?、clear_steam_web_api_key?、steam_id64；头像资源必须通过 AUTH-14 单独上传",
        "200 JSON 管理员身份、Bilibili UID、SteamID64、steam_web_api_key_configured 与掩码",
        "Key 加密保存且不回显；空 Key 保留原值，显式 clear_steam_web_api_key 才删除；凭据变化后清空旧镜像并异步同步"
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
        "200 JSON username、email、avatar_url、bilibili_uid；avatar_url 为 /storage/ 下的 MinIO 资源",
        "服务端校验文件签名后写入公共 MinIO 桶，并原子创建 uploads/assets 记录；替换直接交换唯一文件并将旧对象加入清理队列"
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
        "200 JSON username、email、avatar_url=null、bilibili_uid",
        "只解除当前管理员头像并归档逻辑资源，MinIO 文件继续由素材库管理"
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
    // 说说公开域：HTTP 负责展示和点赞；内容维护走下方后台接口。
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
        "200 分页 items[{id,content,images,like_count,created_at,liked_by_me}]",
        "按 created_at 倒序；未传 visitor_id 时 liked_by_me 恒为 false"
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
        "200 分页 items，meta 包含 counts{watching,finished}、synced_at 和同步状态",
        "只读本地同步镜像，不在请求内访问 Bilibili；非法状态返回 422"
    ),
    endpoint!(
        "MEDIA-02",
        "追番游戏",
        Get,
        "/api/v1/games",
        "/api/v1/games?sort=playtime&page=1&per_page=10",
        Anonymous,
        OK,
        "读取 Steam 游戏进程镜像",
        "Query: status=playing|finished|shelved、recent?、sort=recent|playtime、page/per_page",
        "200 分页 items，包含累计/近两周游玩分钟数和最后游玩时间；meta 包含 total/recent、同步状态",
        "只读本地同步镜像，不在公开请求内访问 Steam；非法状态或排序返回 422；没有详情端点"
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
        "仅返回 approved，avatar_url 由 MinIO 素材文件派生；按 sort_order/created_at 排序；total 用于前台计数"
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
    // 公开灵衣、音乐和看板娘域。
    //
    // 每套灵衣直接拥有完整主题色、封面图文与语音、明暗外观基调。
    // 自动时间段属于站点设置并只引用灵衣；看板娘实现属于后续建设范围。
    endpoint!(
        "RAIMENT-01",
        "灵衣",
        Get,
        "/api/v1/raiments",
        "/api/v1/raiments",
        Anonymous,
        OK,
        "读取公开灵衣目录与站点时间段",
        "无查询参数",
        "200 JSON items[{id,name,cover_url,theme,color_scheme,cover_title,cover_subtitle,cover_character_name,cover_dialogue,cover_voice_label,cover_voice_url?,login_success_voice_url?,kanban_configured}]、schedule{revision,periods[{id,start_at,end_at,raiment_id,playlist_id?}]}、default_raiment_id",
        "封面 URL 可公开访问；不得返回素材 object key、revision、管理字段或内部 object metadata"
    ),
    endpoint!(
        "THEME-02",
        "主题媒体",
        Get,
        "/api/v1/music",
        "/api/v1/music",
        Anonymous,
        OK,
        "读取启用歌单与歌曲引用数据",
        "无查询参数",
        "200 JSON items[{id,name,description,source_kind,tracks[]}]",
        "features.music=false 返回 403；歌单及歌曲按 sort_order/id 排序"
    ),
    // 旧 GET /themes 被 RAIMENT-01 取代并删除。看板娘接口仍是独立待办，
    // 不复用或绕过灵衣已实现的封面/主题持久化。
    endpoint!(
        "KANBAN-02",
        "看板娘",
        Post,
        "/api/v1/kanban/chat",
        "/api/v1/kanban/chat",
        Anonymous,
        OK,
        "代理一次看板娘对话",
        "JSON: session_id、message、article_slug?、raiment_id",
        "200 JSON reply、motion?、egg?、fallback",
        "raiment_id 必须来自 RAIMENT-01 且当前可用；仅据此选择角色视觉上下文，不允许客户端覆盖模型或提示词；服务端引用 kanban_chat 场景；限流 6/分钟；最多回复 3 句；LLM 故障时返回降级台词且 fallback=true"
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
        "默认草稿优先、其次置顶，再按文章日期倒序；筛选枚举非法返回 422；后台可见全部状态"
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
        "JSON: expected_updated_at、title、content_md、category_id、tag_ids、cover_asset_id?、content_asset_ids?、is_pinned、allow_comment、kanban_ref、status?",
        "200 返回 id、slug、status、word_count、read_minutes、updated_at",
        "全量覆盖且事务保存分类/标签/素材引用；expected_updated_at 不匹配返回 409；素材类型必须兼容；status=published 时校验必填字段并首次写 published_at；计算字数和阅读时长"
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
        "事务删除文章及关联标签，并通过 Artalk 页面删除接口清理该文章的全部评论；任一同步失败则返回 502；对象存储素材不在请求内物理删除"
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
        "同一事务处理可执行项；delete 通过 Artalk 页面删除接口清理评论；发布仍逐篇执行发布校验；空数组或未知 action 返回 422"
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
        "200 分页 items，包含 images、like_count、created_at、updated_at",
        "按 created_at 倒序"
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
        "事务替换图片引用且不重置点赞；不存在返回 404"
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
        "事务删除点赞和素材引用；素材本体保留；不存在返回 404"
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
        "至少一个字段；已通过友链必须引用 active 图片素材，公开 avatar_url 由 MinIO 文件派生；外链头像仅作为待审核来源；URL 冲突返回 409"
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
    // 素材库：二进制全部存 MinIO，数据库管理稳定的逻辑素材和引用。
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
        "只返回 active 素材；usable_for 按目标接受类型过滤；列表返回当前文件"
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
        "201 返回素材及文件访问 URL",
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
        "读取素材详情和引用",
        "路径 id",
        "200 JSON asset、references[]、preview",
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
        "/api/v1/admin/assets/{id}/replace",
        "/api/v1/admin/assets/1/replace",
        AdminJwt,
        CREATED,
        "替换素材文件",
        "路径 id；multipart: file；单文件最大 200MB",
        "201 返回更新后的素材",
        "新文件媒体类型必须兼容原素材；替换成功后删除旧数据库记录，旧 MinIO 对象进入异步清理队列，所有逻辑引用自动生效"
    ),
    endpoint!(
        "ASSET-07",
        "素材库",
        Delete,
        "/api/v1/admin/assets/{id}",
        "/api/v1/admin/assets/1",
        AdminJwt,
        NO_CONTENT,
        "删除未被引用的素材",
        "路径 id",
        "204 无响应体",
        "reference_count>0 返回 409；数据库删除与垃圾回收登记处于同一事务，MinIO 清理失败会自动退避重试"
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
        "打包下载素材",
        "JSON: 非空 asset_ids，最多 100 个",
        "200 application/zip 流式响应，Content-Disposition 使用安全文件名",
        "读取各素材文件并从 MinIO 流式打包；限制总展开大小；临时文件必须清理"
    ),
    // 后台灵衣与媒体域。
    //
    // 灵衣用稳定字符串 id 做身份；内置和后续新增的灵衣共用同一套 CRUD。
    // 时间段在站点设置中引用灵衣，不能反向成为灵衣自身的生命周期字段。
    endpoint!(
        "ADMIN-RAIMENT-01",
        "后台灵衣",
        Get,
        "/api/v1/admin/raiments",
        "/api/v1/admin/raiments",
        AdminJwt,
        OK,
        "读取全部灵衣",
        "无查询参数；v1 数据量小，免分页",
        "200 JSON items[{id,name,cover_asset_id,cover_asset,theme,enabled,sort_order,is_default,color_scheme,cover_title,cover_subtitle,cover_character_name,cover_dialogue,cover_voice_label,cover_voice_asset_id?,cover_voice_asset?,login_success_voice_asset_id?,login_success_voice_asset?,kanban_asset_id?,is_builtin,revision,created_at,updated_at}]",
        "素材 URL/文件信息由服务端派生；返回 revision 供写入时做乐观并发控制"
    ),
    endpoint!(
        "ADMIN-RAIMENT-02",
        "后台灵衣",
        Put,
        "/api/v1/admin/raiments/{id}",
        "/api/v1/admin/raiments/saber",
        AdminJwt,
        OK,
        "保存单套灵衣",
        "路径 id 为稳定 slug；JSON: revision、name、cover_asset_id、theme、enabled、sort_order、is_default、color_scheme、cover_title、cover_subtitle、cover_character_name、cover_dialogue、cover_voice_label、cover_voice_asset_id?、login_success_voice_asset_id?、kanban_asset_id?",
        "200 返回规范化后的灵衣与新 revision",
        "只更新目标灵衣，禁止全量覆盖其它灵衣；revision 冲突返回 409；素材必须 active 且类型匹配；id 创建后不可修改"
    ),
    endpoint!(
        "ADMIN-RAIMENT-03",
        "后台灵衣",
        Post,
        "/api/v1/admin/raiments",
        "/api/v1/admin/raiments",
        AdminJwt,
        CREATED,
        "新增灵衣",
        "JSON: name、cover_asset_id、theme、enabled、sort_order、is_default、color_scheme、cover_title、cover_subtitle、cover_character_name、cover_dialogue、cover_voice_label、cover_voice_asset_id?、login_success_voice_asset_id?、kanban_asset_id?",
        "201 返回创建后的完整灵衣；服务端生成稳定 id",
        "封面必须为 active 图片；封面语音和登录成功语音若提供则必须为 active 音频；看板娘字段若提供则必须为 active Live2D 素材"
    ),
    endpoint!(
        "ADMIN-RAIMENT-04",
        "后台灵衣",
        Delete,
        "/api/v1/admin/raiments/{id}",
        "/api/v1/admin/raiments/saber",
        AdminJwt,
        NO_CONTENT,
        "删除灵衣",
        "路径 id 为稳定 slug；JSON: revision",
        "204 无响应体",
        "revision 冲突返回 409；内置灵衣允许删除，但系统必须至少保留一套灵衣；仍被站点时间段引用时返回 409；同步清理封面、语音和看板娘素材引用"
    ),
    endpoint!(
        "ADMIN-RAIMENT-05",
        "后台站点设置",
        Get,
        "/api/v1/admin/site/raiment-schedule",
        "/api/v1/admin/site/raiment-schedule",
        AdminJwt,
        OK,
        "读取站点灵衣时间段",
        "无查询参数",
        "200 JSON revision、periods[{id,start_at,end_at,raiment_id}]",
        "时间使用 24 小时制 HH:MM；允许跨午夜；时间段引用可用灵衣，背景音乐可选引用已启用歌单"
    ),
    endpoint!(
        "ADMIN-RAIMENT-06",
        "后台站点设置",
        Put,
        "/api/v1/admin/site/raiment-schedule",
        "/api/v1/admin/site/raiment-schedule",
        AdminJwt,
        OK,
        "保存站点灵衣时间段",
        "JSON: revision、periods[{id,start_at,end_at,raiment_id,playlist_id?}]",
        "200 返回规范化时间段与新 revision",
        "时间段不可重叠且开始/结束不能相同；灵衣及歌单引用必须存在且启用；revision 冲突返回 409"
    ),
    endpoint!(
        "ADMIN-MUSIC-01",
        "后台主题媒体",
        Get,
        "/api/v1/admin/music",
        "/api/v1/admin/music",
        AdminJwt,
        OK,
        "读取全部歌单摘要",
        "无查询参数且免分页",
        "200 JSON items",
        "按 sort_order/id 返回歌单元数据；本地歌单包含 track_count，曲目通过分页端点按需读取"
    ),
    endpoint!(
        "ADMIN-MUSIC-02",
        "后台主题媒体",
        Post,
        "/api/v1/admin/music",
        "/api/v1/admin/music",
        AdminJwt,
        CREATED,
        "新增歌单",
        "JSON: name?、description、source_kind、external_reference?、enabled",
        "201 返回歌单摘要",
        "本地歌单可引用素材库；网易云与 QQ 歌单创建时验证公开曲目"
    ),
    endpoint!(
        "ADMIN-MUSIC-03",
        "后台主题媒体",
        Put,
        "/api/v1/admin/music/order",
        "/api/v1/admin/music/order",
        AdminJwt,
        OK,
        "批量调整歌单顺序",
        "JSON: order 为无重复歌单 id 数组",
        "200 JSON items 为新顺序",
        "order 必须完整覆盖当前歌单集合；事务更新连续 sort_order，缺失/重复 id 返回 422"
    ),
    endpoint!(
        "ADMIN-MUSIC-04",
        "后台主题媒体",
        Delete,
        "/api/v1/admin/music/{id}",
        "/api/v1/admin/music/1",
        AdminJwt,
        NO_CONTENT,
        "删除歌单",
        "路径参数 id 为正整数",
        "204 无响应体",
        "被灵衣时间段引用的歌单不能删除；删除其它歌单及其本地曲目；素材对象仅解除引用，不在此端点物理删除；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-MUSIC-05",
        "后台主题媒体",
        Put,
        "/api/v1/admin/playlists/{id}",
        "/api/v1/admin/playlists/1",
        AdminJwt,
        OK,
        "更新歌单名称、说明与启用状态",
        "路径参数 id 为正整数；JSON: name、description、enabled",
        "200 返回更新后的歌单摘要",
        "歌单名称不能为空且最多 120 字符；被灵衣时间段引用的歌单不能停用；不存在返回 404"
    ),
    endpoint!(
        "ADMIN-MUSIC-06",
        "后台主题媒体",
        Get,
        "/api/v1/admin/playlists/{id}/tracks",
        "/api/v1/admin/playlists/1/tracks?page=1&per_page=10",
        AdminJwt,
        OK,
        "分页读取指定歌单的歌曲",
        "路径参数 id 为正整数；Query: page>=1、per_page=1..100",
        "200 JSON page、per_page、total、items、status、status_message",
        "本地歌曲通过数据库 LIMIT/OFFSET 分页；外部来源不可用时返回 unavailable 状态与空列表"
    ),
    endpoint!(
        "ADMIN-MUSIC-07",
        "后台主题媒体",
        Post,
        "/api/v1/admin/playlists/{id}/tracks",
        "/api/v1/admin/playlists/1/tracks",
        AdminJwt,
        CREATED,
        "向本地歌单添加素材歌曲",
        "路径参数 id 为正整数；JSON: asset_id、title?、artist、duration_s",
        "201 返回新建歌曲",
        "仅本地歌单可添加素材库中的 active 音频；重复引用返回 409"
    ),
    endpoint!(
        "ADMIN-MUSIC-08",
        "后台主题媒体",
        Delete,
        "/api/v1/admin/playlists/{id}/tracks/{track_id}",
        "/api/v1/admin/playlists/1/tracks/1",
        AdminJwt,
        NO_CONTENT,
        "从本地歌单移除歌曲",
        "路径参数 id、track_id 为正整数",
        "204 无响应体",
        "只删除歌单歌曲引用，不删除素材对象；歌单或歌曲不存在返回 404"
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
    // 后台统一 LLM 凭据域。长期运行的业务场景保存 use_case 引用；文章编辑器
    // 则在每次润色时临时选择已保存的 Key、模型和提示词。
    endpoint!(
        "ADMIN-LLM-01",
        "后台 LLM",
        Get,
        "/api/v1/admin/llm",
        "/api/v1/admin/llm",
        AdminJwt,
        OK,
        "读取统一 LLM 配置与多 Key 列表",
        "无查询参数；connections 为不分页的小规模连接集合",
        "200 JSON connections[{id,display_name,base_url,api_key_configured,status,...}]、use_cases[{connection_id,model,...}]、revision",
        "API Key 永不下发；文章编辑器只能取得连接元数据，并以 connection_id 发起润色"
    ),
    endpoint!(
        "ADMIN-LLM-02",
        "后台 LLM",
        Put,
        "/api/v1/admin/llm",
        "/api/v1/admin/llm",
        AdminJwt,
        OK,
        "保存 Key 状态、删除操作与场景路由",
        "JSON: revision、connections[{id,display_name,base_url,enabled,...}]、use_cases",
        "200 返回规范化连接集合和新 revision，仍不返回 API Key",
        "revision 冲突返回 409；所有启用场景必须显式绑定一个已启用 Key、模型和系统提示词"
    ),
    endpoint!(
        "ADMIN-LLM-05",
        "后台 LLM",
        Post,
        "/api/v1/admin/llm/connections",
        "/api/v1/admin/llm/connections",
        AdminJwt,
        OK,
        "测试并保存新的 LLM Key",
        "JSON: revision、display_name、base_url、api_key；无需也不接受模型",
        "模型列表接口验证成功后返回包含新 Key 的规范化 LLM 配置",
        "验证失败不落库；API Key 加密保存且永不下发；revision 冲突返回 409"
    ),
    endpoint!(
        "ADMIN-LLM-03",
        "后台 LLM",
        Post,
        "/api/v1/admin/llm/test",
        "/api/v1/admin/llm/test",
        AdminJwt,
        OK,
        "重新验证已保存的 LLM Key",
        "JSON: connection_id?；不接受模型或 API Key 草稿",
        "200 JSON reply、latency_ms",
        "通过该 Key 的模型列表接口验证；记录成功/失败、时间和耗时；模型 API 故障返回 502"
    ),
    endpoint!(
        "ADMIN-LLM-04",
        "后台 LLM",
        Post,
        "/api/v1/admin/llm/models",
        "/api/v1/admin/llm/models",
        AdminJwt,
        OK,
        "获取指定 Key 的可用模型",
        "JSON: connection_id?、base_url、api_key?；API Key 只用于本次请求，不落库",
        "200 JSON items[{id,name}]",
        "只读取用户填写 API 地址的 /models 资源；不返回已保存的 API Key"
    ),
    endpoint!(
        "ADMIN-LLM-06",
        "后台 LLM",
        Post,
        "/api/v1/admin/llm/polish",
        "/api/v1/admin/llm/polish",
        AdminJwt,
        OK,
        "使用已保存的 Key 润色文章草稿",
        "JSON: connection_id、model、prompt、target=summary|content、title?、summary、content_md",
        "200 JSON target、text；前端必须先展示原文与候选稿差异，确认后才能替换",
        "仅允许启用且已配置凭据的 Key；Key 明文不下发；摘要最多 120 字，正文保留 Markdown"
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
        "200 JSON 今日访客及差值、文章/草稿/对话/LLM/运行天数",
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
        .filter(|contract| {
            !crate::auth::implements(contract.method, contract.path)
                && !super::assets::implements(contract.method, contract.path)
                && !super::articles::implements(contract.method, contract.path)
                && !super::bangumi::implements(contract.method, contract.path)
                && !super::games::implements(contract.method, contract.path)
                && !super::llm::implements(contract.method, contract.path)
                && !super::playlists::implements(contract.method, contract.path)
                && !super::raiments::implements(contract.method, contract.path)
        })
        .fold(Router::new(), |router, contract| {
            let route = match contract.authentication {
                Authentication::AdminJwt => on(contract.method.filter(), protected_not_implemented),
                Authentication::Anonymous | Authentication::RefreshCookie => {
                    on(contract.method.filter(), not_implemented)
                }
            };
            router.route(
                contract.path,
                route.layer(DefaultBodyLimit::max(contract.max_body_bytes)),
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

async fn protected_not_implemented(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    if !crate::auth::has_valid_admin_session(&state, &headers) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: "invalid_credentials",
                    message: "需要有效的管理员会话".to_owned(),
                },
            }),
        );
    }
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
            artalk_internal_url: None,
            artalk_site_name: "helt.".to_owned(),
            artalk_admin_name: "test".to_owned(),
            artalk_admin_email: "test@example.com".to_owned(),
            artalk_admin_password: "test".to_owned(),
            meting_api_url: None,
            llm_encryption_key_version: 1,
            llm_encryption_secret: "test-llm-encryption-secret-at-least-32-bytes".to_owned(),
            llm_encryption_previous_key_version: None,
            llm_encryption_previous_secret: None,
            llm_private_host_allowlist: Vec::new(),
            public_origin: "http://localhost".to_owned(),
            cors_allowed_origins: vec!["http://localhost:5173".to_owned()],
            request_timeout_secs: 5,
            asset_request_timeout_secs: 300,
            upstream_request_timeout_secs: 15,
        };
        let pool = PgPoolOptions::new()
            .connect_lazy(&config.database_url)
            .expect("valid lazy PostgreSQL URL");
        let state = AppState::new(pool, &config).expect("valid test state");

        build_app(state, &config).expect("contract app should build")
    }

    /// TDD 契约层：固定端点总数、唯一性和每项说明的完整性。
    #[test]
    fn catalog_contains_99_complete_unique_contracts() {
        assert_eq!(ENDPOINT_CONTRACTS.len(), 99);

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
                    && contract.max_body_bytes <= super::ASSET_MULTIPART_BODY_LIMIT_BYTES,
                "{} has an invalid request-body limit",
                contract.id
            );
        }

        let admin_count = ENDPOINT_CONTRACTS
            .iter()
            .filter(|contract| contract.path.starts_with("/api/v1/admin/"))
            .count();
        assert_eq!(admin_count, 80);
        assert_eq!(ENDPOINT_CONTRACTS.len() - admin_count, 19);

        let large_body_endpoints = ENDPOINT_CONTRACTS
            .iter()
            .filter(|contract| contract.max_body_bytes == super::ASSET_MULTIPART_BODY_LIMIT_BYTES)
            .map(|contract| (contract.method.as_str(), contract.path))
            .collect::<HashSet<_>>();
        assert_eq!(
            large_body_endpoints,
            HashSet::from([
                ("POST", "/api/v1/admin/assets"),
                ("POST", "/api/v1/admin/assets/{id}/replace"),
            ])
        );
    }

    /// 固定整个目录的身份快照，避免数量不变时某个方法或路径被悄悄替换。
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
            fingerprint, 8_960_414_761_448_318_737,
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
            "/api/v1/admin/auth/passkey/login/options",
            "/api/v1/admin/auth/passkey/login/verify",
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

    /// TDD 路由层：逐个执行全部代表请求，确认路径和方法已登记。
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

    /// 所有声明为后台会话的端点都必须在无 cookie 时停止在认证/输入边界，
    /// 不能进入真实处理器，也不能让尚未实现的管理端点直接暴露 501。
    #[tokio::test]
    async fn every_admin_jwt_contract_rejects_anonymous_requests() {
        let app = test_app();

        for contract in ENDPOINT_CONTRACTS
            .iter()
            .filter(|contract| contract.authentication == Authentication::AdminJwt)
        {
            let body = match contract.id {
                "AUTH-12" => {
                    r#"{"current_password":"current-password","new_password":"new-password-123"}"#
                }
                "AUTH-13" => r#"{"email":"","bilibili_uid":""}"#,
                _ => "{}",
            };
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(contract.method.http_method())
                        .uri(contract.example_path)
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .expect("valid anonymous admin request"),
                )
                .await
                .expect("admin route should respond");

            assert!(
                !response.status().is_success(),
                "{} {} accepted an anonymous request",
                contract.method.as_str(),
                contract.example_path
            );
            assert_ne!(
                response.status(),
                StatusCode::NOT_IMPLEMENTED,
                "{} {} disclosed an unguarded management placeholder",
                contract.method.as_str(),
                contract.example_path
            );
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
