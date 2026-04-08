mod cli;
mod models;

use std::path::Path;

use clap::Parser;
use config::{Config, Environment, File};
pub use models::*;

use crate::error::GatewayError;

pub struct AppConfig {
    pub gateway: GatewayConfig,
    pub services_config: ServicesConfig,
    pub routes: RoutesConfig,
    pub openapi_spec: String,
}

impl AppConfig {
    pub fn load() -> Result<Self, GatewayError> {
        let cli = cli::Cli::parse();

        let gateway = load_gateway_config(&cli.config_dir, cli.host, cli.port)?;

        let services = load_file_config::<ServicesConfig>(
            &cli.config_dir.join(&gateway.config_files.services),
        )?;

        let routes =
            load_file_config::<RoutesConfig>(&cli.config_dir.join(&gateway.config_files.routes))?;

        let openapi_path = cli.config_dir.join(&gateway.config_files.openapi);
        let openapi_spec = std::fs::read_to_string(&openapi_path)
            .map_err(|e| GatewayError::Config(format!("{}: {}", openapi_path.display(), e)))?;

        Ok(AppConfig {
            gateway,
            services_config: services,
            routes,
            openapi_spec,
        })
    }
}

fn load_gateway_config(
    config_dir: &Path,
    host: Option<String>,
    port: Option<u16>,
) -> Result<GatewayConfig, GatewayError> {
    let mut builder = Config::builder()
        .add_source(File::from(config_dir.join("gateway.toml")))
        .add_source(Environment::with_prefix("GATEWAY").separator("__"));

    if let Some(host) = host {
        builder = builder.set_override("server.host", host)?;
    }

    if let Some(port) = port {
        builder = builder.set_override("server.port", port as i64)?;
    }

    Ok(builder.build()?.try_deserialize()?)
}

fn load_file_config<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, GatewayError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| GatewayError::Config(format!("{}: {}", path.display(), e)))?;

    toml::from_str(&content).map_err(|e| GatewayError::Config(format!("{}: {}", path.display(), e)))
}
