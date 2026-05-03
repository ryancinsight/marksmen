use crate::model::Reference;
use serde::{Deserialize, Serialize};

/// Schema constraint for Cloud Synchronization payloads (WebDAV / S3 / IPFS).
/// This ensures deterministic parsing when merging diverging remote and local libraries.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncPayload {
    /// Universally unique identifier for the device uploading the sync packet
    pub device_id: String,
    
    /// ISO 8601 timestamp of the last successful sync point
    pub last_synced_at: String,
    
    /// A deterministic cryptographic hash (SHA-256) of the library state to verify integrity
    pub library_hash: String,
    
    /// The full collection of references being pushed to the remote
    pub references: Vec<Reference>,

    /// The list of collections/folders mapped
    pub collections: Vec<crate::model::Collection>,
}

use std::collections::{HashMap, HashSet};
use sha2::{Sha256, Digest};

impl SyncPayload {
    /// Create a new sync payload bound to the current UTC timestamp
    pub fn new(device_id: String, mut references: Vec<Reference>, mut collections: Vec<crate::model::Collection>) -> Self {
        // Ensure deterministic ordering for the hash
        references.sort_by(|a, b| a.id.cmp(&b.id));
        collections.sort_by(|a, b| a.id.cmp(&b.id));

        // Compute deterministic SHA-256 hash of the payload state
        let mut hasher = Sha256::new();
        if let Ok(ref_bytes) = serde_json::to_vec(&references) {
            hasher.update(&ref_bytes);
        }
        if let Ok(col_bytes) = serde_json::to_vec(&collections) {
            hasher.update(&col_bytes);
        }
        let result = hasher.finalize();
        let hash = result.iter().map(|b| format!("{:02x}", b)).collect::<String>();

        Self {
            device_id,
            last_synced_at: chrono::Utc::now().to_rfc3339(),
            library_hash: hash,
            references,
            collections,
        }
    }
}

/// Last-Writer-Wins (LWW) resolution engine for Cloud Sync
pub fn merge_sync_payloads(local: SyncPayload, remote: SyncPayload) -> SyncPayload {
    let mut ref_map = HashMap::new();

    // Insert local references
    for r in local.references.into_iter() {
        ref_map.insert(r.id.clone(), r);
    }

    // Merge remote references using LWW
    for r in remote.references.into_iter() {
        match ref_map.get_mut(&r.id) {
            Some(existing) => {
                if r.date_modified > existing.date_modified {
                    *existing = r;
                }
            }
            None => {
                ref_map.insert(r.id.clone(), r);
            }
        }
    }

    let merged_refs: Vec<Reference> = ref_map.into_values().collect();

    // Merge collections
    let mut col_map = HashMap::new();
    for c in local.collections.into_iter() {
        col_map.insert(c.id.clone(), c);
    }

    for c in remote.collections.into_iter() {
        match col_map.get_mut(&c.id) {
            Some(existing) => {
                // Union of ref_ids
                let mut union_set: HashSet<String> = existing.ref_ids.drain(..).collect();
                for id in c.ref_ids {
                    union_set.insert(id);
                }
                let mut new_refs: Vec<String> = union_set.into_iter().collect();
                new_refs.sort();
                existing.ref_ids = new_refs;
            }
            None => {
                col_map.insert(c.id.clone(), c);
            }
        }
    }

    let merged_cols: Vec<crate::model::Collection> = col_map.into_values().collect();

    // Compute the new sync state using the deterministic builder
    SyncPayload::new(local.device_id, merged_refs, merged_cols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_lww() {
        let mut ref1 = Reference::blank();
        ref1.id = "A".to_string();
        ref1.title = "Old Title".to_string();
        ref1.date_modified = "2025-01-01T00:00:00Z".to_string();

        let mut ref1_updated = Reference::blank();
        ref1_updated.id = "A".to_string();
        ref1_updated.title = "New Title".to_string();
        ref1_updated.date_modified = "2025-02-01T00:00:00Z".to_string();

        let mut ref2 = Reference::blank();
        ref2.id = "B".to_string();

        let local = SyncPayload::new("device1".to_string(), vec![ref1], vec![]);
        let remote = SyncPayload::new("device2".to_string(), vec![ref1_updated, ref2], vec![]);

        let merged = merge_sync_payloads(local, remote);
        assert_eq!(merged.references.len(), 2);
        
        let merged_ref1 = merged.references.iter().find(|r| r.id == "A").unwrap();
        assert_eq!(merged_ref1.title, "New Title"); // Remote wins

        // Hashes should be deterministic
        let merged2 = merge_sync_payloads(
            SyncPayload::new("device1".to_string(), vec![merged_ref1.clone()], vec![]),
            SyncPayload::new("device2".to_string(), vec![merged_ref1.clone()], vec![])
        );
        assert_eq!(merged.library_hash, merged2.library_hash);
    }
}
