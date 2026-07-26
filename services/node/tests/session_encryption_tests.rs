use stellpoker_node::crypto::session_encryption::SessionKeyManager;

#[test]
fn test_session_key_management() {
    let mut manager = SessionKeyManager::new();
    
    // Generate a session key
    let session_id = "integration-test-001";
    let keys = manager.generate_session_key(session_id);
    
    // Verify we can retrieve it
    let retrieved = manager.get_session_keys(session_id).unwrap();
    assert_eq!(keys.encryption_key, retrieved.encryption_key);
    
    // Clean up
    manager.cleanup_session(session_id);
    assert!(!manager.session_exists(session_id));
}

#[test]
fn test_multiple_sessions_isolation() {
    let mut manager = SessionKeyManager::new();
    
    let session1 = "session-isolation-1";
    let session2 = "session-isolation-2";
    
    let keys1 = manager.generate_session_key(session1);
    let keys2 = manager.generate_session_key(session2);
    
    // Keys should be different
    assert_ne!(keys1.encryption_key, keys2.encryption_key);
    
    // Session 2 shouldn't see session 1's keys
    let retrieved2 = manager.get_session_keys(session2).unwrap();
    assert_eq!(keys2.encryption_key, retrieved2.encryption_key);
    assert_ne!(keys1.encryption_key, retrieved2.encryption_key);
    
    // Clean one session
    manager.cleanup_session(session1);
    assert!(!manager.session_exists(session1));
    assert!(manager.session_exists(session2));
    
    // Clean the other
    manager.cleanup_session(session2);
    assert!(!manager.session_exists(session2));
}
