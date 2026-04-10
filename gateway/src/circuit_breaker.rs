use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::config::CircuitBreakerConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitState {
    Closed,
    Open { since: Instant },
    HalfOpen,
}

pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    half_open_successes: u32,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            half_open_successes: 0,
            config,
        }
    }

    pub fn allow_request(&mut self) -> bool {
        match &self.state {
            CircuitState::Closed => true,
            CircuitState::Open { since } => {
                let recovery = Duration::from_secs(self.config.recovery_timeout_secs);
                if since.elapsed() >= recovery {
                    self.state = CircuitState::HalfOpen;
                    self.half_open_successes = 0;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.half_open_successes += 1;
                if self.half_open_successes >= self.config.half_open_max_requests {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.half_open_successes = 0;
                }
            }
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            _ => {}
        }
    }

    pub fn record_failure(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open {
                    since: Instant::now(),
                };
                self.half_open_successes = 0;
            }
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitState::Open {
                        since: Instant::now(),
                    };
                }
            }
            _ => {}
        }
    }

    pub fn state(&self) -> &CircuitState {
        &self.state
    }
}

pub type CircuitBreakerRegistry = Arc<Mutex<HashMap<String, CircuitBreaker>>>;

pub fn new_registry() -> CircuitBreakerRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}
