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
    pub health_check: HealthCheckConfig,
}

#[derive(Debug, Deserialize)]
pub struct HealthCheckConfig {
    pub interval_secs: u64,
    pub path: String,
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
    pub timeout_secs: u64,
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

#[derive(Debug, Clone, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout_secs: u64,
    pub half_open_max_requests: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
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
    pub retry: Option<RetryConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}
