use api_gateway::config::AppConfig;
use api_gateway::{Gateway, telemetry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init_tracing("api_gateway=info", std::io::stdout);

    let config = AppConfig::load()?;
    let gateway = Gateway::new(config)?;
    gateway.run().await?;

    Ok(())
}
