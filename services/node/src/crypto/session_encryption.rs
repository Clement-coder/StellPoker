use std::collections::HashMap;
use rand::Rng;
use zeroize::Zeroize;

/// Represents encryption keys for a single session
#[derive(Clone)]
pub struct SessionKeys {
    pub encryption_key: [u8; 32],
    pub session_id: String,
}

impl Drop for SessionKeys {
    fn drop(&mut self) {
        // Securely zero out keys when dropped
        self.encryption_key.zeroize();
    }
}

/// Manages session keys
pub struct SessionKeyManager {
    active_sessions: HashMap<String, SessionKeys>,
}

impl SessionKeyManager {
    pub fn new() -> Self {
        Self {
            active_sessions: HashMap::new(),
        }
    }

    /// Generate a new session key
    pub fn generate_session_key(&mut self, session_id: &str) -> SessionKeys {
        let mut encryption_key = [0u8; 32];
        rand::thread_rng().fill(&mut encryption_key);
        
        let keys = SessionKeys {
            encryption_key,
            session_id: session_id.to_string(),
        };
        
        self.active_sessions.insert(session_id.to_string(), keys.clone());
        keys
    }

    /// Get session keys
    pub fn get_session_keys(&self, session_id: &str) -> Option<&SessionKeys> {
        self.active_sessions.get(session_id)
    }

    /// Clean up session keys
    pub fn cleanup_session(&mut self, session_id: &str) {
        if let Some(keys) = self.active_sessions.remove(session_id) {
            drop(keys); // Keys are zeroized on drop
        }
    }

    /// Check if session exists
    pub fn session_exists(&self, session_id: &str) -> bool {
        self.active_sessions.contains_key(session_id)
    }

    /// Get all active session IDs
    pub fn get_active_sessions(&self) -> Vec<String> {
        self.active_sessions.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_key() {
        let mut manager = SessionKeyManager::new();
        let session_id = "test-session-001";
        
        let keys = manager.generate_session_key(session_id);
        assert_eq!(keys.session_id, session_id);
        assert!(manager.session_exists(session_id));
    }

    #[test]
    fn test_get_session_keys() {
        let mut manager = SessionKeyManager::new();
        let session_id = "test-session-002";
        
        let keys = manager.generate_session_key(session_id);
        let retrieved = manager.get_session_keys(session_id).unwrap();
        
        assert_eq!(keys.encryption_key, retrieved.encryption_key);
    }

    #[test]
    fn test_cleanup_session() {
        let mut manager = SessionKeyManager::new();
        let session_id = "test-session-003";
        
        manager.generate_session_key(session_id);
        assert!(manager.session_exists(session_id));
        
        manager.cleanup_session(session_id);
        assert!(!manager.session_exists(session_id));
    }

    #[test]
    fn test_multiple_sessions() {
        let mut manager = SessionKeyManager::new();
        
        let keys1 = manager.generate_session_key("session-1");
        let keys2 = manager.generate_session_key("session-2");
        
        // Keys should be different
        assert_ne!(keys1.encryption_key, keys2.encryption_key);
        
        let sessions = manager.get_active_sessions();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&"session-1".to_string()));
        assert!(sessions.contains(&"session-2".to_string()));
    }
}
