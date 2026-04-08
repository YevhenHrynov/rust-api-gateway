use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub fn init_tracing<W>(filter: &str, writer: W)
where
    W: for<'writer> MakeWriter<'writer> + Send + Sync + 'static,
{
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|err| {
        eprintln!("invalid RUST_LOG ({err}), falling back to '{filter}'");
        EnvFilter::new(filter)
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().json().with_writer(writer))
        .init();
}
