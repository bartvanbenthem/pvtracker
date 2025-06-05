use kube::api::{Patch, PatchParams};
use kube::{Api, Client, Error, ResourceExt};
use serde_json::{Value, json};
use tracing::*;

use crate::crd::{VolumeTracker, VolumeTrackerStatus};

/// Update VolumeTracker status
pub async fn patch(
    client: Client,
    name: &str,
    namespace: &str,
    success: bool,
) -> Result<VolumeTracker, Error> {
    let api: Api<VolumeTracker> = Api::namespaced(client, namespace);

    let data: Value = json!({
        "status": VolumeTrackerStatus { succeeded: success },
    });

    api.patch_status(name, &PatchParams::default(), &Patch::Merge(&data))
        .await
}

/// Print VolumeTracker status
pub async fn print(client: Client, name: &str, namespace: &str) -> Result<(), Error> {
    let api: Api<VolumeTracker> = Api::namespaced(client, namespace);

    let cdb = api.get_status(name).await?;

    info!(
        "Got status succeeded {:?} for custom resource {} in namespace {}",
        cdb.clone()
            .status
            .unwrap_or(VolumeTrackerStatus { succeeded: false })
            .succeeded,
        cdb.name_any(),
        namespace
    );

    Ok(())
}
