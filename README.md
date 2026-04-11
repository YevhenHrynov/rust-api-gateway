# rust-api-gateway

API Gateway written in Rust. Routes incoming HTTP requests to backend services based on config files, with support for retries, circuit breakers, rate limiting, per-route timeouts, health checks, and Swagger UI.

## Project structure

```
gateway/          - the gateway itself
order-service/    - mock order backend (port 3001)
payment-service/  - mock payment backend (port 3002)
```

Both mock services are minimal axum apps that return hardcoded data. They exist so you can run and test the gateway locally without setting up real backends.

## How to run

Start all three in separate terminals:

```sh
cargo run -p order-service
cargo run -p payment-service
cargo run -p api-gateway
```

The gateway listens on `http://localhost:8080`

## Configuration

All config lives in `gateway/config/`:

- `gateway.toml` - server settings, timeouts, health check interval
- `services.toml` - list of backend services and their URLs
- `routes.toml` - route definitions with per-route features
- `openapi.yaml` - OpenAPI spec served by Swagger UI

You can point to a different config directory:

```sh
cargo run -p api-gateway -- --config-dir /path/to/config
```

### Route config example

```toml
[[routes]]
path = "/api/v1/orders"
method = "GET"
service = "order-service"
upstream_path = "/v1/orders"
timeout_secs = 10                  # optional, overrides global timeout

[routes.retry]                     # optional
max_retries = 3
base_delay_ms = 200

[routes.circuit_breaker]           # optional
failure_threshold = 3
recovery_timeout_secs = 10
half_open_max_requests = 3

[routes.rate_limit]                # optional
points = 3
duration = 1                       # seconds, defaults to 1
```

## Features

### Reverse proxy
Routes requests to backend services based on method + path matching. Supports path parameters (`:id` style) that get rewritten to the upstream path.

### Retries
Configurable per route. Uses exponential backoff (`base_delay_ms * 2^attempt`). Only retries on 5xx responses or connection errors.

### Circuit breaker
Per-service circuit breaker with three states: closed, open, half-open. When a service starts failing, the circuit opens and requests get rejected immediately with 503 instead of piling up on a dead backend.

### Rate limiting
Per-IP token bucket rate limiter, configurable per route. Idle buckets get cleaned up automatically.

### Per-route timeouts
Each route can have its own `timeout_secs`. If not set, falls back to the global `timeout_secs` from `gateway.toml`. Returns 504 on timeout.

### Health checks
The gateway periodically pings each backend's health endpoint. Unhealthy services get 503 responses without actually forwarding the request. Health status is available at `GET /health`.

### Swagger UI
Served at `/swagger-ui/` from the OpenAPI spec in the config directory.