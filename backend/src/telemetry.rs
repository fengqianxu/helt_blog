use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));

    let registry = tracing_subscriber::registry().with(filter);
    let result = if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        registry.with(fmt::layer().json()).try_init()
    } else {
        registry.with(fmt::layer().compact()).try_init()
    };

    let _ = result;
}
