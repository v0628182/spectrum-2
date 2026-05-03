//! Online-only gate — VanySound requires an active internet connection.
//!
//! On startup, pings the API to confirm connectivity.
//! If offline, sets a shared flag that the frontend reads to block usage.
//! Re-checks periodically so the app recovers when connectivity returns.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// API endpoint to check connectivity (lightweight, already exists).
const CONNECTIVITY_URL: &str = "https://api.vanysound.com/api/version";

/// How often to re-check when offline (every 15 seconds).
const OFFLINE_RETRY_INTERVAL: Duration = Duration::from_secs(15);

/// How often to re-check when online (every 5 minutes).
const ONLINE_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Shared flag: true = online, false = offline.
static ONLINE: AtomicBool = AtomicBool::new(false);

/// Check if the app has confirmed internet connectivity.
pub fn is_online() -> bool {
    ONLINE.load(Ordering::Relaxed)
}

/// Ping the API to verify connectivity.
fn check_connectivity() -> bool {
    match ureq::get(CONNECTIVITY_URL)
        .timeout(Duration::from_secs(8))
        .call()
    {
        Ok(response) => response.status() == 200,
        Err(_) => false,
    }
}

/// Spawn the background connectivity monitor thread.
/// Returns an Arc<AtomicBool> for online status + the JoinHandle.
pub fn spawn_connectivity_monitor(
    shutdown: Arc<AtomicBool>,
) -> (Arc<AtomicBool>, thread::JoinHandle<()>) {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    let shutdown_clone = shutdown;

    let join = thread::Builder::new()
        .name("connectivity-monitor".into())
        .spawn(move || {
            // Initial check with 3 retries
            let mut online = false;
            for attempt in 1..=3 {
                if shutdown_clone.load(Ordering::Relaxed) {
                    return;
                }
                tracing::info!("Connectivity check attempt {}/3", attempt);
                if check_connectivity() {
                    online = true;
                    break;
                }
                if attempt < 3 {
                    thread::sleep(Duration::from_secs(3));
                }
            }

            flag_clone.store(online, Ordering::SeqCst);
            ONLINE.store(online, Ordering::SeqCst);

            if online {
                tracing::info!("Connectivity: ONLINE");
            } else {
                tracing::warn!("Connectivity: OFFLINE — app will be restricted");
            }

            // Continuous monitoring loop — exits on shutdown signal
            loop {
                if shutdown_clone.load(Ordering::Relaxed) {
                    tracing::info!("Connectivity monitor: shutdown signal, exiting");
                    break;
                }

                let interval = if ONLINE.load(Ordering::Relaxed) {
                    ONLINE_CHECK_INTERVAL
                } else {
                    OFFLINE_RETRY_INTERVAL
                };

                // Sleep in short increments so we can check shutdown
                let sleep_step = Duration::from_secs(1);
                let mut slept = Duration::ZERO;
                while slept < interval {
                    if shutdown_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(sleep_step);
                    slept += sleep_step;
                }

                let result = check_connectivity();
                let was_online = ONLINE.load(Ordering::Relaxed);

                flag_clone.store(result, Ordering::SeqCst);
                ONLINE.store(result, Ordering::SeqCst);

                if result && !was_online {
                    tracing::info!("Connectivity restored — app fully functional");
                } else if !result && was_online {
                    tracing::warn!("Connectivity lost — app entering restricted mode");
                }
            }
        })
        .expect("Failed to spawn connectivity monitor");

    (flag, join)
}
