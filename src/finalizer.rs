use kube::api::{Patch, PatchParams};
use kube::{Api, Client, Error};
use serde_json::{Value, json};

use anyhow::Result;
use k8s_openapi::NamespaceResourceScope;
use kube::Resource;
use serde::de::DeserializeOwned;
use serde_json;
use std::fmt::Debug;

///let _ = add_finalizer_namespaced_resource::<PersistentVolumeClaim>(
///    client.clone(),
///    "test-pvc",
///    "default",
///    "volumetrackers.cndev.nl/finalizer",
///)
///.await?;
pub async fn add_finalizer_namespaced_resource<T>(
    client: Client,
    name: &str,
    namespace: &str,
    value: &str,
) -> Result<T, Error>
where
    T: Clone
        + Debug
        + Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + DeserializeOwned,
{
    let api: Api<T> = Api::namespaced(client, namespace);
    let finalizer: Value = json!({
        "metadata": {
            "finalizers": [value]
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&finalizer);
    api.patch(name, &PatchParams::default(), &patch).await
}

///let _ = add_finalizer_cluster_resource::<PersistentVolume>(
///    client.clone(),
///    "test-pv",
///    "volumetrackers.cndev.nl/finalizer",
///)
///.await?;
pub async fn add_finalizer_cluster_resource<T>(
    client: Client,
    name: &str,
    value: &str,
) -> Result<T, Error>
where
    T: Clone
        + Debug
        + Resource<DynamicType = (), Scope = kube::core::ClusterResourceScope>
        + DeserializeOwned,
{
    let api: Api<T> = Api::all(client);
    let finalizer: Value = json!({
        "metadata": {
            "finalizers": [value]
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&finalizer);
    api.patch(name, &PatchParams::default(), &patch).await
}

///let _ = delete_finalizer_namespaced_resource::<PersistentVolumeClaim>(
///    client.clone(),
///    "test-pvc",
///    "default",
///)
///.await?;
pub async fn delete_finalizer_namespaced_resource<T>(
    client: Client,
    name: &str,
    namespace: &str,
) -> Result<T, Error>
where
    T: Clone
        + Debug
        + Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + DeserializeOwned,
{
    let api: Api<T> = Api::namespaced(client, namespace);
    let finalizer: Value = json!({
        "metadata": {
            "finalizers": null
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&finalizer);
    api.patch(name, &PatchParams::default(), &patch).await
}

///let _ = add_finalizer_cluster_resource::<PersistentVolume>(
///    client.clone(),
///    "test-pv",
///)
///.await?;
pub async fn delete_finalizer_cluster_resource<T>(client: Client, name: &str) -> Result<T, Error>
where
    T: Clone
        + Debug
        + Resource<DynamicType = (), Scope = kube::core::ClusterResourceScope>
        + DeserializeOwned,
{
    let api: Api<T> = Api::all(client);
    let finalizer: Value = json!({
        "metadata": {
            "finalizers": null
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&finalizer);
    api.patch(name, &PatchParams::default(), &patch).await
}
