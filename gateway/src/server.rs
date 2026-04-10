use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use hyper::Request;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use utoipa_swagger_ui::Config;

use crate::circuit_breaker::{self, CircuitBreakerRegistry};
use crate::config::{AppConfig, HealthCheckConfig};
use crate::error::GatewayError;
use crate::health::{self, HealthState};
use crate::proxy::{self, ProxyClient, ResponseBody};
use crate::router::Router;

const DOCS_BASE_PATH: &str = "/swagger-ui";
const DOCS_SPEC_PATH: &str = "/swagger-ui/openapi.yaml";
const DOCS_REDIRECT_PATH: &str = "/swagger-ui/";

pub struct Gateway {
    router: Router,
    proxy: ProxyClient,
    services: HashMap<String, String>,
    addr: SocketAddr,
    openapi_spec: String,
    swagger_config: Arc<Config<'static>>,
    health_state: HealthState,
    health_check_config: HealthCheckConfig,
    timeout: Duration,
    circuit_breakers: CircuitBreakerRegistry,
}

impl Gateway {
    pub fn new(config: AppConfig) -> Result<Self, GatewayError> {
        let services: HashMap<String, String> = config
            .services_config
            .services
            .into_iter()
            .map(|s| (s.name, s.url.trim_end_matches('/').to_owned()))
            .collect();

        for route in &config.routes.routes {
            if !services.contains_key(&route.service) {
                return Err(GatewayError::Config(format!(
                    "route '{}' references unknown service '{}'",
                    route.path, route.service
                )));
            }
        }

        let router = Router::new(config.routes.routes);
        let proxy = ProxyClient::new();

        let addr: SocketAddr = format!(
            "{}:{}",
            config.gateway.server.host, config.gateway.server.port
        )
        .parse()
        .map_err(|e| GatewayError::Config(format!("invalid server address: {}", e)))?;

        let swagger_config = Arc::new(Config::from(DOCS_SPEC_PATH));

        let service_names: Vec<String> = services.keys().cloned().collect();
        let health_state = health::new_health_state(&service_names);
        let circuit_breakers = circuit_breaker::new_registry();

        Ok(Gateway {
            router,
            proxy,
            services,
            addr,
            openapi_spec: config.openapi_spec,
            swagger_config,
            health_state,
            health_check_config: config.gateway.health_check,
            timeout: Duration::from_secs(config.gateway.server.timeout_secs),
            circuit_breakers,
        })
    }

    pub async fn run(self) -> Result<(), GatewayError> {
        let listener = TcpListener::bind(self.addr).await?;
        info!("listening on {}", self.addr);

        let checker = health::HealthChecker::new(
            self.services.clone(),
            self.health_state.clone(),
            self.health_check_config.path.clone(),
            Duration::from_secs(self.health_check_config.interval_secs),
            self.timeout,
        );
        checker.spawn();

        let shared = Arc::new(self);

        loop {
            let (stream, remote_addr) = tokio::select! {
                result = listener.accept() => match result {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("accept error: {e}");
                        continue;
                    }
                },
                Ok(()) = tokio::signal::ctrl_c() => {
                    info!("shutdown signal received, stopping");
                    return Ok(());
                }
            };
            let gateway = Arc::clone(&shared);

            tokio::spawn(async move {
                let io = TokioIo::new(stream);

                let service = service_fn(move |req: Request<Incoming>| {
                    let gw = Arc::clone(&gateway);
                    async move { gw.handle(req, remote_addr).await }
                });

                if let Err(e) = Builder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await
                {
                    error!("connection error from {}: {}", remote_addr, e);
                }
            });
        }
    }

    async fn handle(
        &self,
        req: Request<Incoming>,
        remote_addr: SocketAddr,
    ) -> Result<http::Response<ResponseBody>, Infallible> {
        match self.route(req, remote_addr).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                error!("internal error: {e}");
                Ok(proxy::json_error_fallback(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error",
                ))
            }
        }
    }

    async fn route(
        &self,
        req: Request<Incoming>,
        remote_addr: SocketAddr,
    ) -> Result<http::Response<ResponseBody>, GatewayError> {
        let method = req.method().clone();
        let path = req.uri().path().to_owned();

        info!("{} {} {}", remote_addr, method, path);

        if path == "/health" && method == http::Method::GET {
            let summary = health::build_health_summary(&self.health_state).await;
            let json =
                serde_json::to_string(&summary).map_err(|e| GatewayError::Proxy(e.to_string()))?;
            return proxy::json_response(&json);
        }

        if path == DOCS_BASE_PATH {
            return proxy::redirect(DOCS_REDIRECT_PATH);
        }

        if path == DOCS_SPEC_PATH {
            return proxy::yaml_response(&self.openapi_spec);
        }

        if let Some(response) = self.serve_swagger_ui(&path) {
            return Ok(response);
        }

        let route_match = match self.router.match_request(&method, &path) {
            Some(m) => m,
            None => return proxy::not_found(),
        };

        let upstream_base = match self.services.get(&route_match.service_name) {
            Some(url) => url,
            None => return proxy::bad_gateway("service not configured"),
        };

        {
            let health = self.health_state.read().await;
            if let Some(status) = health.get(&route_match.service_name) {
                if !status.is_healthy() {
                    warn!(
                        service = %route_match.service_name,
                        "request rejected: service unhealthy"
                    );
                    return proxy::service_unavailable(&route_match.service_name);
                }
            }
        }

        if let Some(cb_config) = &route_match.circuit_breaker {
            let mut breakers = self.circuit_breakers.lock().await;
            let cb = breakers
                .entry(route_match.service_name.clone())
                .or_insert_with(|| circuit_breaker::CircuitBreaker::new(cb_config.clone()));

            if !cb.allow_request() {
                warn!(
                    service = %route_match.service_name,
                    "request rejected: circuit breaker open"
                );
                return proxy::circuit_open(&route_match.service_name);
            }
        }

        let result = match &route_match.retry {
            Some(retry_config) => {
                self.proxy
                    .forward_with_retry(
                        req,
                        upstream_base,
                        &route_match.upstream_path,
                        retry_config,
                    )
                    .await
            }
            None => {
                self.proxy
                    .forward(req, upstream_base, &route_match.upstream_path)
                    .await
            }
        };

        match result {
            Ok(resp) => {
                if route_match.circuit_breaker.is_some() {
                    let mut breakers = self.circuit_breakers.lock().await;
                    if let Some(cb) = breakers.get_mut(&route_match.service_name) {
                        if resp.status().is_server_error() {
                            cb.record_failure();
                        } else {
                            cb.record_success();
                        }
                    }
                }

                info!(
                    "{} {} -> {} {} ({})",
                    remote_addr,
                    method,
                    resp.status().as_u16(),
                    route_match.upstream_path,
                    route_match.service_name,
                );
                Ok(resp)
            }
            Err(e) => {
                if route_match.circuit_breaker.is_some() {
                    let mut breakers = self.circuit_breakers.lock().await;
                    if let Some(cb) = breakers.get_mut(&route_match.service_name) {
                        cb.record_failure();
                    }
                }

                error!(
                    "{} {} -> {} error: {}",
                    remote_addr, method, route_match.service_name, e
                );
                proxy::bad_gateway("upstream connection failed")
            }
        }
    }

    fn serve_swagger_ui(&self, path: &str) -> Option<http::Response<ResponseBody>> {
        let tail = match path.strip_prefix(DOCS_BASE_PATH) {
            Some(rest) => rest.strip_prefix('/').unwrap_or(rest),
            None => return None,
        };

        let api_doc = utoipa_swagger_ui::serve(tail, self.swagger_config.clone()).ok()??;

        let response = http::Response::builder()
            .status(http::StatusCode::OK)
            .header(http::header::CONTENT_TYPE, api_doc.content_type)
            .body(proxy::bytes_body(api_doc.bytes.into_owned()))
            .ok()?;

        Some(response)
    }
}
