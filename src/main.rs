use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::api::storage::v1::StorageClass;

use pvtracker::crd::VolumeTracker;
use pvtracker::resource::*;
use pvtracker::status;

use chrono::Utc;
use futures::stream::StreamExt;
use k8s_openapi::api::core::v1::{PersistentVolume, PersistentVolumeClaim};
use kube::Config as KubeConfig;
use kube::ResourceExt;
use kube::runtime::watcher::Config;
use kube::{Api, client::Client, runtime::Controller, runtime::controller::Action};
use pvtracker::utils;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::sleep;
use tracing::*;


use kube_runtime::WatchStreamExt;
use kube_runtime::watcher;
use futures::TryStreamExt;
use tokio::task;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Context injected with each `reconcile` and `on_error` method invocation.
struct ContextData {
    /// Kubernetes client to make Kubernetes API requests with. Required for K8S resource management.
    client: Client,
}

impl ContextData {
    /// Constructs a new instance of ContextData.
    ///
    /// # Arguments:
    /// - `client`: A Kubernetes client to make Kubernetes REST API requests with. Resources
    /// will be created and deleted with this client.
    pub fn new(client: Client) -> Self {
        ContextData { client }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    // First, a Kubernetes client must be obtained using the `kube` crate
    // Attempt to infer KubeConfig
    let config = KubeConfig::infer()
        .await
        .map_err(|e| kube::Error::InferConfig(e))?;
    let kubeconfig: Client = Client::try_from(config)?;

    // Preparation of resources used by the `kube_runtime::Controller`
    let crd_api: Api<VolumeTracker> = Api::all(kubeconfig.clone());
    let context: Arc<ContextData> = Arc::new(ContextData::new(kubeconfig.clone()));

    // Wait until there is at least one `VolumeTracker` on the cluster before continuing
    loop {
        let volume_trackers = crd_api.list(&Default::default()).await?;

        // If there is at least one VolumeTracker, proceed
        if !volume_trackers.items.is_empty() {
            break;
        }

        // Otherwise, log and wait for a bit before checking again
        info!("No VolumeTracker found, waiting to start controller...");
        sleep(Duration::from_secs(10)).await;
    }

    let (tx, rx) = mpsc::channel::<()>(16); // channel to trigger global reconciles
    let signal_stream = ReceiverStream::new(rx); // converts mpsc into a stream

    // Start the PV watcher in background
    start_pv_watcher(kubeconfig.clone(), tx).await?;

    // The controller comes from the `kube_runtime` crate and manages the reconciliation process.
    // It requires the following information:
    // - `kube::Api<T>` this controller "owns". In this case, `T = VolumeTracker`, as this controller owns the `VolumeTracker` resource,
    // - `kube::runtime::watcher::Config` can be adjusted for precise filtering of `VolumeTracker` resources before the actual reconciliation, e.g. by label,
    // - `reconcile` function with reconciliation logic to be called each time a resource of `VolumeTracker` kind is created/updated/deleted,
    // - `on_error` function to call whenever reconciliation fails.
    Controller::new(crd_api.clone(), Config::default())
        .shutdown_on_signal()
        .reconcile_all_on(signal_stream) 
        .run(reconcile, on_error, context)
        .for_each(|reconciliation_result| async move {
            match reconciliation_result {
                Ok(custom_resource) => {
                    info!("Reconciliation successful. Resource: {:?}", custom_resource);
                }
                Err(reconciliation_err) => {
                    warn!("Reconciliation error: {:?}", reconciliation_err)
                }
            }
        })
        .await;

    Ok(())
}

async fn reconcile(cr: Arc<VolumeTracker>, context: Arc<ContextData>) -> Result<Action, Error> {
    let client: Client = context.client.clone(); // The `Client` is shared -> a clone from the reference is obtained

    // The resource of `VolumeTracker` kind is required to have a namespace set. However, it is not guaranteed
    // the resource will have a `namespace` set. Therefore, the `namespace` field on object's metadata
    // is optional and Rust forces the programmer to check for it's existence first.
    let namespace: String = match cr.namespace() {
        None => {
            // If there is no namespace to deploy to defined, reconciliation ends with an error immediately.
            return Err(Error::UserInputError(
                "Expected VolumeTracker resource to be namespaced. Can't deploy to an unknown namespace."
                    .to_owned(),
            ));
        }
        // If namespace is known, proceed. In a more advanced version of the operator, perhaps
        // the namespace could be checked for existence first.
        Some(namespace) => namespace,
    };

    let name = cr.name_any(); // Name of the VolumeTracker resource is used to name the subresources as well.
    let tracker = cr.as_ref();

    let now = Utc::now();
    //let tf = now.format("%Y-%m-%d-%H%M%S");
    let tf = now.timestamp();

    let cluster_name =
        utils::get_most_common_cluster_name(client.clone(), &tracker.spec.cluster_name_key).await?;

    // Run all resource log writes as async tasks in parallel
    let (pv_result, pvc_result, sc_result, ns_result) = tokio::join!(
        fetch_and_write_resource::<PersistentVolume>(
            client.clone(),
            &cr.spec.mount_path,
            &cluster_name,
            "persistent_volumes.log",
            &tf
        ),
        fetch_and_write_resource::<PersistentVolumeClaim>(
            client.clone(),
            &cr.spec.mount_path,
            &cluster_name,
            "persistent_volume_claims.log",
            &tf
        ),
        fetch_and_write_resource::<StorageClass>(
            client.clone(),
            &cr.spec.mount_path,
            &cluster_name,
            "storage_classes.log",
            &tf
        ),
        fetch_and_write_resource::<Namespace>(
            client.clone(),
            &cr.spec.mount_path,
            &cluster_name,
            "namespaces.log",
            &tf
        )
    );

    // display errors if any
    match (pv_result, pvc_result, sc_result, ns_result) {
        (Ok(_), Ok(_), Ok(_), Ok(_)) => {
            info!("All resources retrieved and written successfully.");
            status::patch(client.clone(), &name, &namespace, true).await?;
        }
        (pv, pvc, sc, ns) => {
            if let Err(e) = pv {
                error!("Failed to process PersistentVolumes: {:?}", e);
            }
            if let Err(e) = pvc {
                error!("Failed to process PersistentVolumeClaims: {:?}", e);
            }
            if let Err(e) = sc {
                error!("Failed to process StorageClasses: {:?}", e);
            }
            if let Err(e) = ns {
                error!("Failed to process Namespaces: {:?}", e);
            }
            // Optionally fail the program if any failed
            return Err(Error::UserInputError(
                "One or more resource operations failed".to_string(),
            ));
        }
    }

    status::print(client.clone(), &name, &namespace).await?;

    Ok(Action::requeue(Duration::from_secs(32000)))
}

fn on_error(cr: Arc<VolumeTracker>, error: &Error, context: Arc<ContextData>) -> Action {
    let client = context.client.clone();
    let name = cr.name_any();
    let namespace = cr.namespace().unwrap_or_else(|| "default".to_string());

    error!(
        error = ?error,
        name = %name,
        namespace = %namespace,
        "Reconciliation error occurred"
    );

    // Spawn async patch inside sync function
    tokio::spawn(async move {
        if let Err(e) = status::patch(client, &name, &namespace, false).await {
            error!("Failed to update status: {:?}", e);
        }
    });

    // Requeue to try again later
    Action::requeue(Duration::from_secs(5))
}

/// All errors possible to occur during reconciliation
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Any error originating from the `kube-rs` crate
    #[error("Kubernetes reported error: {source}")]
    KubeError {
        #[from]
        source: kube::Error,
    },

    /// Any error originating from the watcher
    #[error("Watcher reported error: {source}")]
    WatcherError {
        #[from]
        source: kube_runtime::watcher::Error,
    },

    /// Error in user input or VolumeTracker resource definition, typically missing fields.
    #[error("Invalid VolumeTracker CRD: {0}")]
    UserInputError(String),

    /// Catch-all for any other error.
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/*
async fn start_pv_watcher(kube_client: Client) -> Result<(), Error> {
    let api = Api::<PersistentVolume>::all(kube_client);

    // Spawn the watcher in a new Tokio task
    task::spawn(async move {
        let result = watcher(api, Config::default())
            .applied_objects()
            .default_backoff()
            .try_for_each(|p| async move {
                info!("saw {}", p.name_any());
                Ok::<_, kube_runtime::watcher::Error>(())
            })
            .await;

        if let Err(e) = result {
            // Handle watcher error (or return it somewhere)
            error!("Watcher failed: {:?}", e);
        }
    });

    Ok(())
}
    */

async fn start_pv_watcher(kube_client: Client, tx: mpsc::Sender<()>) -> Result<(), anyhow::Error> {
    let pv_api = Api::<PersistentVolume>::all(kube_client.clone());

    task::spawn(async move {
        let result = watcher(pv_api, Config::default())
            .applied_objects()
            .default_backoff()
            .try_for_each(|pv| {
                let tx = tx.clone();
                async move {
                    info!("PV changed: {}", pv.name_any());
                    tx.send(()).await.ok();
                    Ok(())
                }
            })
            .await;

        if let Err(err) = result {
            error!("PV watcher failed: {:?}", err);
        }
    });

    Ok(())
}

/*
/// Watch PersistentVolumes and trigger all reconciles
async fn start_pv_watcher(client: Client, tx: mpsc::Sender<()>) -> anyhow::Result<()> {
    let pv_api = Api::<kube::api::Resource>::all(client);

    task::spawn(async move {
        let result = watcher(pv_api, Config::default())
            .applied_objects()
            .default_backoff()
            .try_for_each(|pv| {
                let tx = tx.clone();
                async move {
                    info!("PV changed: {}", pv.name_any());

                    // Send signal to trigger all reconciliations
                    if tx.send(()).await.is_err() {
                        error!("Failed to send reconcile trigger");
                    }

                    Ok(())
                }
            })
            .await;

        if let Err(err) = result {
            error!("PV watcher failed: {:?}", err);
        }
    });

    Ok(())
}

*/