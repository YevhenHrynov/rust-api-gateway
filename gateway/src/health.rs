use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::Empty;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Healthy,
    Unhealthy(String),
    Unknown,
}

impl ServiceStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, ServiceStatus::Healthy)
    }
}

pub type HealthState = Arc<RwLock<HashMap<String, ServiceStatus>>>;

pub fn new_health_state(service_names: &[String]) -> HealthState {
    let map: HashMap<String, ServiceStatus> = service_names
        .iter()
        .map(|name| (name.clone(), ServiceStatus::Unknown))
        .collect();
    Arc::new(RwLock::new(map))
}

pub struct HealthChecker {
    services: HashMap<String, String>,
    state: HealthState,
    path: String,
    interval: Duration,
    timeout: Duration,
}

impl HealthChecker {
    pub fn new(
        services: HashMap<String, String>,
        state: HealthState,
        path: String,
        interval: Duration,
        timeout: Duration,
    ) -> Self {
        Self {
            services,
            state,
            path,
            interval,
            timeout,
        }
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let client = Client::builder(TokioExecutor::new()).build(HttpConnector::new());

            let mut interval = tokio::time::interval(self.interval);

            loop {
                interval.tick().await;

                for (name, url) in &self.services {
                    let status = check_service(&client, url, &self.path, self.timeout).await;

                    match &status {
                        ServiceStatus::Healthy => {
                            info!(service = %name, "healthy");
                        }
                        ServiceStatus::Unhealthy(reason) => {
                            warn!(service = %name, reason = %reason, "unhealthy");
                        }
                        ServiceStatus::Unknown => {}
                    }

                    self.state.write().await.insert(name.clone(), status);
                }
            }
        })
    }
}

#[derive(Debug, Serialize)]
pub struct HealthSummary {
    pub status: String,
    pub services: HashMap<String, String>,
}

fn build_health_summary_from_map(map: &HashMap<String, ServiceStatus>) -> HealthSummary {
    let services: HashMap<String, String> = map
        .iter()
        .map(|(name, status)| {
            let label = match status {
                ServiceStatus::Healthy => "healthy".to_owned(),
                ServiceStatus::Unhealthy(reason) => format!("unhealthy: {reason}"),
                ServiceStatus::Unknown => "unknown".to_owned(),
            };
            (name.clone(), label)
        })
        .collect();

    let all_healthy = map.values().all(ServiceStatus::is_healthy);

    HealthSummary {
        status: if all_healthy { "healthy" } else { "degraded" }.to_string(),
        services,
    }
}

pub async fn build_health_summary(state: &HealthState) -> HealthSummary {
    let map = state.read().await;
    build_health_summary_from_map(&map)
}

type HealthClient = Client<HttpConnector, Empty<Bytes>>;

async fn check_service(
    client: &HealthClient,
    base_url: &str,
    path: &str,
    timeout: Duration,
) -> ServiceStatus {
    let uri = match format!("{}{}", base_url, path).parse::<http::Uri>() {
        Ok(u) => u,
        Err(e) => return ServiceStatus::Unhealthy(e.to_string()),
    };

    let req = match http::Request::builder()
        .method(http::Method::GET)
        .uri(uri)
        .body(Empty::<Bytes>::new())
    {
        Ok(r) => r,
        Err(e) => return ServiceStatus::Unhealthy(e.to_string()),
    };

    match tokio::time::timeout(timeout, client.request(req)).await {
        Ok(Ok(resp)) if resp.status().is_success() => ServiceStatus::Healthy,
        Ok(Ok(resp)) => ServiceStatus::Unhealthy(format!("status {}", resp.status())),
        Ok(Err(e)) => ServiceStatus::Unhealthy(e.to_string()),
        Err(_) => ServiceStatus::Unhealthy("timeout".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_status_is_healthy() {
        assert!(ServiceStatus::Healthy.is_healthy());
        assert!(!ServiceStatus::Unhealthy("timeout".to_owned()).is_healthy());
        assert!(!ServiceStatus::Unknown.is_healthy());
    }

    #[test]
    fn new_health_state_initializes_all_unknown() {
        let names = vec!["order-service".to_owned(), "payment-service".to_owned()];
        let state = new_health_state(&names);
        let map = state.blocking_read();

        assert_eq!(map.len(), 2);
        assert_eq!(map["order-service"], ServiceStatus::Unknown);
        assert_eq!(map["payment-service"], ServiceStatus::Unknown);
    }

    #[test]
    fn health_summary_reports_overall_status() {
        let state = new_health_state(&["order-service".to_owned(), "payment-service".to_owned()]);
        {
            let mut map = state.blocking_write();
            map.insert("order-service".to_owned(), ServiceStatus::Healthy);
            map.insert("payment-service".to_owned(), ServiceStatus::Healthy);
        }
        let summary = {
            let map = state.blocking_read();
            build_health_summary_from_map(&map)
        };
        assert_eq!(summary.status, "healthy");
        assert_eq!(summary.services["order-service"], "healthy");

        {
            let mut map = state.blocking_write();
            map.insert(
                "payment-service".to_owned(),
                ServiceStatus::Unhealthy("down".to_owned()),
            );
        }
        let summary = {
            let map = state.blocking_read();
            build_health_summary_from_map(&map)
        };
        assert_eq!(summary.status, "degraded");
        assert_eq!(summary.services["payment-service"], "unhealthy: down");
    }

    #[tokio::test]
    async fn check_service_reports_unhealthy_on_connection_refused() {
        let client = Client::builder(TokioExecutor::new()).build(HttpConnector::new());

        let status = check_service(
            &client,
            "http://127.0.0.1:8181",
            "/health",
            Duration::from_secs(1),
        )
        .await;

        assert!(!status.is_healthy());
    }
}
