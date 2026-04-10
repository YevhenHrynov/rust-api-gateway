use std::collections::HashMap;

use http::Method;

use crate::config::{CircuitBreakerConfig, RateLimitConfig, RetryConfig, RouteDefinition};

#[derive(Debug)]
pub struct Router {
    routes: Vec<CompiledRoute>,
}

pub struct RouteMatch {
    pub service_name: String,
    pub upstream_path: String,
    pub retry: Option<RetryConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug)]
struct CompiledRoute {
    segments: Vec<Segment>,
    method: Method,
    service_name: String,
    upstream_pattern: String,
    retry: Option<RetryConfig>,
    circuit_breaker: Option<CircuitBreakerConfig>,
    rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug)]
enum Segment {
    Static(String),
    Param(String),
}

impl Router {
    pub fn new(definitions: Vec<RouteDefinition>) -> Self {
        let routes = definitions
            .into_iter()
            .map(|def| {
                let segments = def
                    .path
                    .trim_matches('/')
                    .split('/')
                    .map(|s| {
                        if let Some(name) = s.strip_prefix(':') {
                            Segment::Param(name.to_owned())
                        } else {
                            Segment::Static(s.to_owned())
                        }
                    })
                    .collect();

                CompiledRoute {
                    segments,
                    method: def.method,
                    service_name: def.service,
                    upstream_pattern: def.upstream_path,
                    retry: def.retry,
                    circuit_breaker: def.circuit_breaker,
                    rate_limit: def.rate_limit,
                }
            })
            .collect();

        Router { routes }
    }

    pub fn match_request(&self, method: &Method, path: &str) -> Option<RouteMatch> {
        let path = path.split('?').next().unwrap_or(path);
        let request_segments: Vec<&str> = path.trim_matches('/').split('/').collect();

        self.routes.iter().find_map(|route| {
            if route.method != *method || route.segments.len() != request_segments.len() {
                return None;
            }

            let mut params = HashMap::new();
            let all_matched =
                route
                    .segments
                    .iter()
                    .zip(request_segments.iter())
                    .all(|(segment, &actual)| match segment {
                        Segment::Static(expected) => expected == actual,
                        Segment::Param(name) => {
                            params.insert(name.as_str(), actual);
                            true
                        }
                    });

            if !all_matched {
                return None;
            }

            let upstream_path = params
                .iter()
                .fold(route.upstream_pattern.clone(), |path, (key, value)| {
                    path.replace(&format!(":{key}"), value)
                });

            Some(RouteMatch {
                service_name: route.service_name.clone(),
                upstream_path,
                retry: route.retry.clone(),
                circuit_breaker: route.circuit_breaker.clone(),
                rate_limit: route.rate_limit.clone(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RouteDefinition;

    fn test_routes() -> Vec<RouteDefinition> {
        vec![
            RouteDefinition {
                path: "/api/v1/orders".to_owned(),
                method: Method::GET,
                service: "order-service".to_owned(),
                upstream_path: "/v1/orders".to_owned(),
                retry: None,
                circuit_breaker: None,
                rate_limit: None,
            },
            RouteDefinition {
                path: "/api/v1/orders".to_owned(),
                method: Method::POST,
                service: "order-service".to_owned(),
                upstream_path: "/v1/orders".to_owned(),
                retry: None,
                circuit_breaker: None,
                rate_limit: None,
            },
            RouteDefinition {
                path: "/api/v1/orders/:id".to_owned(),
                method: Method::GET,
                service: "order-service".to_owned(),
                upstream_path: "/v1/orders/:id".to_owned(),
                retry: None,
                circuit_breaker: None,
                rate_limit: None,
            },
            RouteDefinition {
                path: "/api/v1/orders/:id".to_owned(),
                method: Method::DELETE,
                service: "order-service".to_owned(),
                upstream_path: "/v1/orders/:id".to_owned(),
                retry: None,
                circuit_breaker: None,
                rate_limit: None,
            },
            RouteDefinition {
                path: "/api/v1/orders/:id".to_owned(),
                method: Method::PUT,
                service: "order-service".to_owned(),
                upstream_path: "/v1/orders/:id".to_owned(),
                retry: None,
                circuit_breaker: None,
                rate_limit: None,
            },
        ]
    }

    fn router() -> Router {
        Router::new(test_routes())
    }

    #[test]
    fn static_route_matches() {
        let m = router()
            .match_request(&Method::POST, "/api/v1/orders")
            .unwrap();
        assert_eq!(m.service_name, "order-service");
        assert_eq!(m.upstream_path, "/v1/orders");
    }

    #[test]
    fn param_route_matches_and_rewrites() {
        let m = router()
            .match_request(&Method::GET, "/api/v1/orders/abc-123")
            .unwrap();
        assert_eq!(m.service_name, "order-service");
        assert_eq!(m.upstream_path, "/v1/orders/abc-123");
    }

    #[test]
    fn wrong_method_returns_none() {
        assert!(
            router()
                .match_request(&Method::PATCH, "/api/v1/orders")
                .is_none()
        );
    }

    #[test]
    fn unknown_path_returns_none() {
        assert!(
            router()
                .match_request(&Method::GET, "/api/v1/unknown")
                .is_none()
        );
    }

    #[test]
    fn same_path_different_methods() {
        let r = router();

        let m = r
            .match_request(&Method::DELETE, "/api/v1/orders/1")
            .unwrap();
        assert_eq!(m.upstream_path, "/v1/orders/1");

        let m = r.match_request(&Method::PUT, "/api/v1/orders/1").unwrap();
        assert_eq!(m.upstream_path, "/v1/orders/1");
    }

    #[test]
    fn query_string_ignored_during_matching() {
        let m = router()
            .match_request(&Method::POST, "/api/v1/orders?debug=true")
            .unwrap();
        assert_eq!(m.upstream_path, "/v1/orders");
    }

    #[test]
    fn invalid_method_rejected_at_deserialization() {
        let toml_str = r#"
            [[routes]]
            path = "/test"
            method = ""
            service = "svc"
            upstream_path = "/test"
        "#;
        let result = toml::from_str::<crate::config::RoutesConfig>(toml_str);
        assert!(result.is_err());
    }
}
