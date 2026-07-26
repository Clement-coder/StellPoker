# MPC Session Encryption

## Overview
Each MPC session uses unique encryption keys to secure communication between nodes.

## Implementation

### SessionKeyManager
The `SessionKeyManager` manages encryption keys for active MPC sessions.

### Key Generation
- Each session gets a unique 32-byte encryption key
- Keys are generated using a cryptographically secure RNG
- Keys are stored in memory only

### Session Lifecycle
1. **Creation**: Session key generated when session starts
2. **Usage**: Keys used for encrypting/decrypting messages
3. **Cleanup**: Keys zeroized when session ends

### Security Features
- **Unique Keys**: Each session has its own key
- **Isolation**: Sessions cannot access other sessions' keys
- **Secure Cleanup**: Keys are zeroized on drop
- **No Persistence**: Keys never written to disk

## Usage Example

```rust
use stellpoker_node::crypto::session_encryption::SessionKeyManager;

let mut manager = SessionKeyManager::new();

// Start session
let session_id = "mpc-session-001";
let keys = manager.generate_session_key(session_id);

// Use session key for encryption
let encryption_key = &keys.encryption_key;

// End session
manager.cleanup_session(session_id);
