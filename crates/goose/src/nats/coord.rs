//! NATS-native coordination: a distributed claim/lease over the bus so
//! concurrent goose instances (and their subagents) cooperate on shared
//! resources instead of racing — they announce ownership and see a live view,
//! rather than silently clobbering the same file or drifting on stale state.
//!
//! Built on JetStream KV (`Store::create` is an atomic compare-and-set, so a
//! claim is a true lock; the bucket's `max_age` is the lease TTL and a heartbeat
//! renews it; on owner death the key expires and others can take over).
//!
//! Like the firehose it is **opt-in, non-blocking, and safe-by-default**:
//!   - active only when `GOOSE_NATS_URL` is set AND `GOOSE_NATS_COORD` is truthy;
//!   - when off (the common single-user case) `claim()` returns an instant
//!     no-op granted lease — zero behavior change;
//!   - if the broker lacks JetStream, it logs once and degrades to no-op granted
//!     (never hard-fails a turn);
//!   - every broker call is bounded by a short timeout.

use crate::config::Config;
use async_nats::jetstream;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Duration;
use tokio::sync::OnceCell;

const COORD_BUCKET: &str = "goose_coord";
/// Default lease TTL (bucket `max_age`); a heartbeat renews at half this.
pub const DEFAULT_LEASE_TTL: Duration = Duration::from_secs(60);
/// How long a file write waits for a contended claim before proceeding anyway.
pub const FILE_CLAIM_MAX_WAIT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// Claims currently held by this process, keyed by KV key. A second claim on a
/// resource this process already holds shares the same [`Active`] (and so the
/// same KV entry and heartbeat) instead of getting an untracked no-op lease;
/// the entry is released only when the last holder drops.
static LOCAL_CLAIMS: LazyLock<Mutex<HashMap<String, Weak<Active>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Coordination is on only when NATS is configured AND `GOOSE_NATS_COORD` is
/// truthy in either the environment or config.yaml.
fn coord_enabled() -> bool {
    let config = Config::global();
    let url_ok = config
        .get_text_param("GOOSE_NATS_URL")
        .map(|u| !u.trim().is_empty())
        .unwrap_or(false);
    url_ok && config.get_flag("GOOSE_NATS_COORD")
}

struct Coordinator {
    store: jetstream::kv::Store,
    client: async_nats::Client,
    instance: String,
    prefix: String,
    ttl: Duration,
}

impl Coordinator {
    /// Fire-and-forget coord event so `nats sub '<prefix>.coord.>'` shows a live
    /// view of claims/releases across the fleet.
    fn publish(&self, kind: &'static str, resource: &str) {
        let subject = format!("{}.{}", self.prefix, kind);
        let env = json!({
            "v": 1,
            "type": kind,
            "instance": self.instance,
            "ts": chrono::Utc::now().to_rfc3339(),
            "resource": resource,
        });
        if let Ok(b) = serde_json::to_vec(&env) {
            let client = self.client.clone();
            tokio::spawn(async move {
                let _ = client.publish(subject, bytes::Bytes::from(b)).await;
            });
        }
    }
}

static COORD: OnceCell<Option<Arc<Coordinator>>> = OnceCell::const_new();

async fn coordinator() -> Option<Arc<Coordinator>> {
    COORD.get_or_init(init_coordinator).await.clone()
}

async fn init_coordinator() -> Option<Arc<Coordinator>> {
    let config = Config::global();
    let url: String = config.get_param("GOOSE_NATS_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    let prefix: String = config
        .get_param("GOOSE_NATS_SUBJECT")
        .ok()
        .filter(|s: &String| !s.trim().is_empty())
        .unwrap_or_else(|| super::DEFAULT_SUBJECT_PREFIX.to_string());
    let instance = super::instance_identity(config);

    let client = match tokio::time::timeout(CONNECT_TIMEOUT, async_nats::connect(&url)).await {
        Ok(Ok(c)) => c,
        _ => {
            tracing::warn!(
                "NATS coord disabled: connect to {} failed or timed out",
                url
            );
            return None;
        }
    };
    let js = jetstream::new(client.clone());
    let kv_config = jetstream::kv::Config {
        bucket: COORD_BUCKET.to_string(),
        description: "goose-plus agent/subagent claim-lease coordination".to_string(),
        history: 1,
        max_age: DEFAULT_LEASE_TTL,
        ..Default::default()
    };
    let store = match tokio::time::timeout(CONNECT_TIMEOUT, js.create_key_value(kv_config)).await {
        Ok(Ok(s)) => s,
        _ => {
            tracing::warn!(
                "NATS coord disabled: JetStream KV unavailable (start the broker with `nats-server -js`)"
            );
            return None;
        }
    };
    tracing::info!(
        "NATS coordination enabled (bucket '{}', instance '{}', lease {:?})",
        COORD_BUCKET,
        instance,
        DEFAULT_LEASE_TTL
    );
    Some(Arc::new(Coordinator {
        store,
        client,
        instance,
        prefix,
        ttl: DEFAULT_LEASE_TTL,
    }))
}

/// A held claim. Dropping (or `release`) frees the resource for other instances
/// once the last holder in this process is done with it. A no-op lease
/// (coordination off / degraded) does nothing on drop.
pub struct Lease {
    inner: Option<Arc<Active>>,
}

struct Active {
    coord: Arc<Coordinator>,
    key: String,
    resource: String,
    heartbeat: tokio::task::JoinHandle<()>,
    released: AtomicBool,
}

impl Active {
    /// Claim the right to release, exactly once across `release()` and `Drop`.
    /// Returns the pieces needed to delete the KV entry, or `None` if another
    /// path already took ownership of the release.
    fn take_release(&self) -> Option<(Arc<Coordinator>, String, String)> {
        if self.released.swap(true, Ordering::AcqRel) {
            return None;
        }
        self.heartbeat.abort();
        if let Ok(mut claims) = LOCAL_CLAIMS.lock() {
            claims.remove(&self.key);
        }
        Some((self.coord.clone(), self.key.clone(), self.resource.clone()))
    }
}

/// Fires when the *last* holder of a shared claim drops it.
impl Drop for Active {
    fn drop(&mut self) {
        if let Some((coord, key, resource)) = self.take_release() {
            tokio::spawn(async move {
                let _ = coord.store.delete(&key).await;
                coord.publish("coord.release", &resource);
            });
        }
    }
}

impl Lease {
    fn noop() -> Self {
        Lease { inner: None }
    }

    fn active(coord: Arc<Coordinator>, key: String, resource: String, revision: u64) -> Self {
        let c = coord.clone();
        let k = key.clone();
        let interval = (coord.ttl / 2).max(Duration::from_secs(1));
        let heartbeat = tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            tick.tick().await; // first tick is immediate
            let mut revision = revision;
            loop {
                tick.tick().await;
                let value =
                    json!({ "instance": c.instance, "ts": chrono::Utc::now().to_rfc3339() });
                let Ok(b) = serde_json::to_vec(&value) else {
                    continue;
                };
                // Renew against the revision we last wrote. A plain `put` would
                // silently overwrite whoever re-claimed the key after our lease
                // expired (heartbeat stalled past the TTL); the compare-and-set
                // fails instead, and we stop renewing a claim we no longer hold.
                match c.store.update(&k, bytes::Bytes::from(b), revision).await {
                    Ok(next) => revision = next,
                    Err(e) => {
                        tracing::warn!(
                            "nats coord: lost claim on '{}' ({}); stopping heartbeat",
                            k,
                            e
                        );
                        return;
                    }
                }
            }
        });
        let active = Arc::new(Active {
            coord,
            key: key.clone(),
            resource,
            heartbeat,
            released: AtomicBool::new(false),
        });
        if let Ok(mut claims) = LOCAL_CLAIMS.lock() {
            claims.insert(key, Arc::downgrade(&active));
        }
        Lease {
            inner: Some(active),
        }
    }

    /// Share a claim this process already holds, if it is still live.
    fn share(key: &str) -> Option<Self> {
        let claims = LOCAL_CLAIMS.lock().ok()?;
        let active = claims.get(key)?.upgrade()?;
        Some(Lease {
            inner: Some(active),
        })
    }

    /// Explicitly release the claim (also happens on drop, best-effort).
    /// A no-op while another holder of the same claim is still alive.
    pub async fn release(mut self) {
        let Some(active) = self.inner.take() else {
            return;
        };
        if Arc::strong_count(&active) > 1 {
            return;
        }
        if let Some((coord, key, resource)) = active.take_release() {
            let _ = coord.store.delete(&key).await;
            coord.publish("coord.release", &resource);
        }
    }
}

/// Try to claim `resource` for this instance.
///
/// Returns `Some(lease)` when granted (acquired, or a no-op lease when
/// coordination is off / degraded / already owned by this instance), and `None`
/// only when the resource is actively held by **another** live instance.
pub async fn claim(resource: &str) -> Option<Lease> {
    if !coord_enabled() {
        return Some(Lease::noop());
    }
    let Some(coord) = coordinator().await else {
        return Some(Lease::noop()); // broker/JetStream unavailable: degrade to granted
    };
    let key = super::sanitize_token(resource);
    let value = json!({
        "instance": coord.instance,
        "resource": resource,
        "ts": chrono::Utc::now().to_rfc3339(),
    });
    let body = bytes::Bytes::from(serde_json::to_vec(&value).unwrap_or_default());

    match tokio::time::timeout(CONNECT_TIMEOUT, coord.store.create(&key, body)).await {
        Ok(Ok(revision)) => {
            coord.publish("coord.claim", resource);
            Some(Lease::active(
                coord.clone(),
                key,
                resource.to_string(),
                revision,
            ))
        }
        Ok(Err(_)) => {
            // Key exists. If this process still holds it, hand back a share of
            // that same claim so the KV entry outlives the first holder's drop
            // — a no-op lease here would let the entry be deleted while this
            // caller is still mutating the resource.
            if let Some(shared) = Lease::share(&key) {
                return Some(shared);
            }
            // No live local holder. If the entry is nonetheless ours it is a
            // stale leftover (e.g. a delete that has not landed yet), so grant
            // rather than stall the turn on a resource nobody is holding.
            if let Ok(Ok(Some(entry))) =
                tokio::time::timeout(CONNECT_TIMEOUT, coord.store.get(&key)).await
            {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&entry) {
                    if v.get("instance").and_then(|x| x.as_str()) == Some(coord.instance.as_str()) {
                        return Some(Lease::noop());
                    }
                }
            }
            None
        }
        Err(_) => Some(Lease::noop()), // timed out: don't stall the turn
    }
}

/// Cooperative claim used on the hot path: try to claim, and if another instance
/// holds it, wait up to `max_wait` for release, then proceed anyway (a returned
/// lease is always granted). Never blocks a lone instance.
pub async fn claim_or_wait(resource: &str, max_wait: Duration) -> Lease {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        if let Some(lease) = claim(resource).await {
            return lease;
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(
                "nats coord: '{}' still held by another instance after {:?}; proceeding",
                resource,
                max_wait
            );
            return Lease::noop();
        }
        tracing::debug!(
            "nats coord: '{}' held by another instance; waiting",
            resource
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Install the file-coordination hook into goose-mcp so the developer tools
/// claim a file before mutating it. Idempotent; a no-op fast-path when
/// coordination is off. Called once by goose at startup. Respects the crate
/// dependency direction (goose → goose-mcp): goose-mcp defines the hook, goose
/// installs the implementation here.
pub fn install_file_coordinator() {
    goose_mcp::coord_hook::set_file_claim(Box::new(|path: String| {
        Box::pin(async move {
            let lease = claim_or_wait(&path, FILE_CLAIM_MAX_WAIT).await;
            Box::new(lease) as goose_mcp::coord_hook::Guard
        })
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_matches_conventional_spellings() {
        for v in ["1", "true", "TRUE", " yes ", "on"] {
            assert!(Config::flag_is_truthy(v), "{v:?} should be truthy");
        }
        for v in ["0", "false", "no", "off", "", "2"] {
            assert!(!Config::flag_is_truthy(v), "{v:?} should not be truthy");
        }
    }

    #[tokio::test]
    async fn claim_is_noop_granted_when_disabled() {
        // No GOOSE_NATS_URL / GOOSE_NATS_COORD in the test env → instant grant,
        // no broker contacted, drop is a no-op.
        let lease = claim("/tmp/example/file.rs").await;
        assert!(
            lease.is_some(),
            "claim must grant a no-op lease when coord is off"
        );
        lease.unwrap().release().await;
    }
}
