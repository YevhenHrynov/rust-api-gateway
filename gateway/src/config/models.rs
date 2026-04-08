use http::Method;
use serde::Deserialize;

fn deserialize_method<'de, D>(deserializer: D) -> Result<Method, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<Method>()
        .map_err(|_| serde::de::Error::custom(format!("invalid HTTP method '{s}'")))
}

#[derive(Debug, Deserialize)]
pub struct GatewayConfig {
    pub server: ServerConfig,
    pub config_files: ConfigFiles,
}

#[derive(Debug, Deserialize)]
pub struct ConfigFiles {
    pub services: String,
    pub routes: String,
    pub openapi: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
pub struct ServicesConfig {
    pub services: Vec<ServiceDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceDefinition {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct RoutesConfig {
    pub routes: Vec<RouteDefinition>,
}

#[derive(Debug, Deserialize)]
pub struct RouteDefinition {
    pub path: String,
    #[serde(deserialize_with = "deserialize_method")]
    pub method: Method,
    pub service: String,
    pub upstream_path: String,
}
