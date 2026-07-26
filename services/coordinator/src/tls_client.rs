// TLS client for coordinator (Issue #93)
pub struct TlsConfig { pub enabled: bool }
impl TlsConfig { pub fn from_env() -> Self { Self { enabled: false } } }
