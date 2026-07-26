// Heartbeat sender for MPC nodes (Issue #94)
use std::time::Duration;

pub fn spawn_heartbeat_sender(node_id: u32, coordinator_url: String) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/api/mpc/heartbeat/{}", coordinator_url, node_id);
        let interval = Duration::from_secs(10);
        
        loop {
            tokio::time::sleep(interval).await;
            match client.post(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!("Heartbeat sent successfully");
                }
                Ok(resp) => {
                    tracing::warn!("Heartbeat failed: {}", resp.status());
                }
                Err(e) => {
                    tracing::error!("Heartbeat error: {}", e);
                }
            }
        }
    });
}
