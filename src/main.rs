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
use kube::runtime::watcher::Config as WatcherConfig;
use kube::{Api, client::Client, runtime::Controller, runtime::controller::Action};
use pvtracker::utils;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::sleep;
use tracing::*;

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

    // Setting the timeout in the Watcher config mitigates the issue of not receiving
    // events after some time because of silently dropped HTTP connections (not closed)
    // this is a quick fix, a better solution would be to add a timeout if an event
    // hasn't been received in N minutes (you can tune N depending on your workload).
    // The network has to be seen as unreliable to expect that HTTP/TCP connections
    // will stay alive for a long time. (Waiting for this feature in kube-rs release)
    let watcher_config = WatcherConfig {
        timeout: Some(200),
        ..Default::default()
    };

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

    // Getting all the specified storage resources on a cluster level
    let pvc_api = Api::<PersistentVolumeClaim>::all(kubeconfig.clone());
    let pv_api = Api::<PersistentVolume>::all(kubeconfig.clone());
    let sc_api = Api::<StorageClass>::all(kubeconfig.clone());

    // get all the `ObjectRef`s for all the custom resources on the custer.
    let volume_tracker_refs =
        utils::make_object_refs::<VolumeTracker>(kubeconfig.clone(), None).await?;
    // create an atomic ref counted custom resource object reference vector
    let volume_tracker_refs_arc = Arc::new(volume_tracker_refs);

    // map the specified storage resources `ObjectRef` to the CR `ObjectRef`
    // this in relation to the watches functionality on the storage resource and CR
    let pv_mapper = utils::make_object_ref_mapper::<PersistentVolume, VolumeTracker>(
        volume_tracker_refs_arc.clone(),
    );
    let pvc_mapper = utils::make_object_ref_mapper::<PersistentVolumeClaim, VolumeTracker>(
        volume_tracker_refs_arc.clone(),
    );
    let sc_mapper = utils::make_object_ref_mapper::<StorageClass, VolumeTracker>(
        volume_tracker_refs_arc.clone(),
    );

    // The controller comes from the `kube_runtime` crate and manages the reconciliation process.
    // It requires the following information:
    // - `kube::Api<T>` this controller "owns". In this case, `T = VolumeTracker`, as this controller owns the `VolumeTracker` resource,
    // - `kube::runtime::watcher::Config` can be adjusted for precise filtering of `VolumeTracker` resources before the actual reconciliation, e.g. by label,
    // - `reconcile` function with reconciliation logic to be called each time a resource of `VolumeTracker` kind is created/updated/deleted,
    // - `on_error` function to call whenever reconciliation fails.
    Controller::new(crd_api.clone(), Config::default())
        .watches(pv_api, watcher_config.clone(), pv_mapper)
        .watches(pvc_api, watcher_config.clone(), pvc_mapper)
        .watches(sc_api, watcher_config.clone(), sc_mapper)
        .shutdown_on_signal()
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
    /// Error in user input or VolumeTracker resource definition, typically missing fields.
    #[error("Invalid VolumeTracker CRD: {0}")]
    UserInputError(String),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
