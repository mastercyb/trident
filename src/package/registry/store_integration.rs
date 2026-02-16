use crate::hash::ContentHash;
use crate::store::{Codebase, Definition};

use super::client::RegistryClient;
use super::types::*;

pub fn publish_codebase(
    codebase: &Codebase,
    client: &RegistryClient,
    tags: &[String],
) -> Result<Vec<PublishResult>, String> {
    let names = codebase.list_names();
    let mut results = Vec::new();

    for (name, hash) in &names {
        let def = match codebase.lookup_hash(hash) {
            Some(d) => d,
            None => continue,
        };

        let pub_def = PublishedDefinition {
            hash: hash.to_hex(),
            source: def.source.clone(),
            module: def.module.clone(),
            is_pub: def.is_pub,
            params: def.params.clone(),
            return_ty: def.return_ty.clone(),
            dependencies: def.dependencies.iter().map(|h| h.to_hex()).collect(),
            requires: def.requires.clone(),
            ensures: def.ensures.clone(),
            name: Some(name.to_string()),
            tags: tags.to_vec(),
            verified: false,
            verification_cert: None,
        };

        match client.publish(&pub_def) {
            Ok(result) => results.push(result),
            Err(e) => {
                return Err(format!("failed to publish '{}': {}", name, e));
            }
        }
    }

    Ok(results)
}

/// Pull a definition from a registry into the local store.
pub fn pull_into_codebase(
    codebase: &mut Codebase,
    client: &RegistryClient,
    name_or_hash: &str,
) -> Result<PullResult, String> {
    let pull = if name_or_hash.len() == 64 && name_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        client.pull(name_or_hash)?
    } else {
        client.pull_by_name(name_or_hash)?
    };

    let hash = ContentHash::from_hex(&pull.hash)
        .ok_or_else(|| "invalid hash in pull response".to_string())?;

    if codebase.lookup_hash(&hash).is_some() {
        return Ok(pull);
    }

    let deps: Vec<ContentHash> = pull
        .dependencies
        .iter()
        .filter_map(|h| ContentHash::from_hex(h))
        .collect();

    let def = Definition {
        source: pull.source.clone(),
        module: pull.module.clone(),
        is_pub: true,
        params: pull.params.clone(),
        return_ty: pull.return_ty.clone(),
        dependencies: deps,
        requires: pull.requires.clone(),
        ensures: pull.ensures.clone(),
        first_seen: crate::package::unix_timestamp(),
    };

    codebase.store_definition(hash, def);

    if name_or_hash.len() != 64 || !name_or_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        codebase.bind_name(name_or_hash, hash);
    }

    codebase.save().map_err(|e| e.to_string())?;

    Ok(pull)
}
