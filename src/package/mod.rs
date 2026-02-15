pub mod cache;
#[allow(dead_code)]
pub mod hash;
pub mod manifest;
pub mod poseidon2;
pub mod registry;
pub mod store;

/// Current Unix timestamp in seconds (shared utility for store, registry, cache).
pub(crate) fn unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
