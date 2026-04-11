use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::config::RateLimitConfig;

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: u32) -> Self {
        Self {
            tokens: capacity as f64,
            last_refill: Instant::now(),
        }
    }

    fn is_full(&self, capacity: u32) -> bool {
        self.tokens >= capacity as f64
    }

    fn try_acquire(&mut self, capacity: u32, refill_rate: f64) -> bool {
        self.refill(capacity, refill_rate);

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self, capacity: u32, refill_rate: f64) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * refill_rate).min(capacity as f64);
        self.last_refill = now;
    }
}

pub type RateLimiterRegistry = Arc<Mutex<HashMap<String, RateLimiter>>>;

const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);
const IDLE_THRESHOLD: Duration = Duration::from_secs(60);

pub fn new() -> RateLimiterRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn spawn_cleanup(registry: RateLimiterRegistry) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        loop {
            interval.tick().await;
            let mut limiters = registry.lock().await;
            for limiter in limiters.values_mut() {
                limiter.cleanup_idle_buckets();
            }
        }
    });
}

pub struct RateLimiter {
    buckets: HashMap<IpAddr, TokenBucket>,
    points: u32,
    refill_rate: f64,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let refill_rate = config.points as f64 / config.duration as f64;
        Self {
            buckets: HashMap::new(),
            points: config.points,
            refill_rate,
        }
    }

    pub fn try_acquire(&mut self, ip: IpAddr) -> bool {
        let bucket = self
            .buckets
            .entry(ip)
            .or_insert_with(|| TokenBucket::new(self.points));

        bucket.try_acquire(self.points, self.refill_rate)
    }

    fn cleanup_idle_buckets(&mut self) {
        let points = self.points;
        self.buckets.retain(|_, bucket| {
            let idle = bucket.last_refill.elapsed() >= IDLE_THRESHOLD;
            !(idle && bucket.is_full(points))
        });
    }
}
