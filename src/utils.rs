use anyhow::{Result, anyhow};
use k8s_openapi::Metadata;
use k8s_openapi::NamespaceResourceScope;
use k8s_openapi::api::core::v1::Node;
use kube::Resource;
use kube::{Api, Client, api::ListParams};
use kube_runtime::reflector::ObjectRef;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json;
use std::collections::HashMap;
use std::fs::{File, create_dir_all};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use tracing::*;

use std::fmt::Debug;

/// Generate `ObjectRef`s for all instances of a given Kubernetes resource type.
pub async fn make_object_refs<T>(
    client: Client,
    namespace: Option<&str>,
) -> Result<Vec<ObjectRef<T>>>
where
    T: Clone
        + Debug
        + Resource<DynamicType = (), Scope = NamespaceResourceScope>
        + DeserializeOwned
        + Serialize
        + Send
        + Sync
        + 'static,
{
    let api: Api<T> = match namespace {
        Some(ns) => Api::namespaced(client, ns),
        None => Api::all(client),
    };

    let mut refs: Vec<ObjectRef<T>> = Vec::new();
    let resources = api.list(&ListParams::default()).await?;

    for resource in resources.items {
        let metadata = resource.meta();
        let name = metadata
            .name
            .clone()
            .ok_or_else(|| anyhow!("Missing metadata.name"))?;
        let ns = metadata
            .namespace
            .clone()
            .unwrap_or_else(|| "default".to_string());

        info!("Resource '{}' has namespace: {}", name, ns);

        refs.push(ObjectRef::new(&name).within(&ns));
    }

    Ok(refs)
}

//pub fn make_object_ref_mapper<T>(
//    refs: Arc<Vec<ObjectRef<VolumeTracker>>>,
//) -> impl Fn(T) -> Vec<ObjectRef<VolumeTracker>> {
//    move |_: T| (*refs).clone()
//}
pub fn make_object_ref_mapper<T, CR>(
    refs: Arc<Vec<ObjectRef<CR>>>,
) -> impl Fn(T) -> Vec<ObjectRef<CR>>
where
    CR: Clone + Resource<DynamicType = ()> + 'static,
    T: Metadata + 'static,
{
    move |_: T| (*refs).clone()
}

/// Writes a list of serializable items to a file in JSONL format.
pub async fn write_json_to_file<T>(items: &[T], file_name: &str) -> Result<(), anyhow::Error>
where
    T: Serialize,
{
    let file = create_file_with_dirs(file_name)?;
    let mut writer = BufWriter::new(file);

    for item in items {
        let json_line = serde_json::to_string(item)?;
        writeln!(writer, "{}", json_line)?;
    }

    info!("Items written to {}, one per line", file_name);
    Ok(())
}

fn create_file_with_dirs(file_name: &str) -> std::io::Result<File> {
    if let Some(parent) = Path::new(file_name).parent() {
        create_dir_all(parent)?;
    }

    File::create(file_name)
}

/// Get the most common cluter name based on a given annotation key among all nodes
pub async fn get_most_common_cluster_name(
    client: Client,
    annotation_key: &str,
) -> Result<String, anyhow::Error> {
    let nodes: Api<Node> = Api::all(client);
    let node_list = nodes.list(&ListParams::default()).await?;

    let mut freq_cn: HashMap<String, usize> = HashMap::new();

    for node in node_list.items {
        if let Some(annotations) = &node.metadata.annotations {
            if let Some(cluster_name) = annotations.get(annotation_key) {
                *freq_cn.entry(cluster_name.clone()).or_insert(0) += 1;
            }
        }
    }

    let (most_common, _) = freq_cn
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .ok_or_else(|| anyhow!("No cluster names found in node annotations"))?;

    Ok(most_common)
}
