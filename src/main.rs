use anyhow::Result;
use kube::Client;
use tracing::{info, warn};

use outrider::config::Config;
use outrider::controllers::{cluster::ClusterReconciler, secret::SecretReconciler};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Outrider operator");

    // Load configuration
    let config = Config::from_env()?;
    info!(
        "Configuration loaded: default_target_namespace={}",
        config.default_target_namespace
    );

    // Create Kubernetes client
    let client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

    // Start both controllers concurrently
    let secret_controller = SecretReconciler::new(client.clone(), config.clone());
    let cluster_controller = ClusterReconciler::new(client.clone(), config.clone());

    info!("Starting controllers...");

    // Run both controllers concurrently, both must succeed
    tokio::try_join!(
        secret_controller.run(),
        cluster_controller.run()
    )?;

    // This should never be reached as controllers run forever
    warn!("All controllers stopped unexpectedly");
    Ok(())
}