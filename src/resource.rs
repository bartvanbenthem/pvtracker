use crate::utils;
use anyhow::Result;
use k8s_openapi::Metadata;
use kube::Resource;
use kube::api::ObjectList;
use kube::core::ObjectMeta;
use kube::{Api, Client, api::ListParams};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use std::path::Path;

// Generic function to read a specific resource type and return a list
async fn get_resource_list<T>(client: Client) -> Result<ObjectList<T>, anyhow::Error>
where
    T: Clone
        + Debug
        + Resource<DynamicType = ()>
        + Metadata<Ty = ObjectMeta>
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
{
    // Access the Resource <T> API (it's cluster-scoped)
    let r: Api<T> = Api::all(client);

    // List Resource <T>
    let lp = ListParams::default();
    let r_list = r.list(&lp).await?;

    Ok(r_list)
}

// Generic function to fetch and write a resource in json format to disk
pub async fn fetch_and_write_resource<T>(
    client: Client,
    mount_path: &str,
    cluster_name: &str,
    file_name: &str,
    tf: &i64,
) -> Result<(), anyhow::Error>
where
    T: Clone
        + Debug
        + Resource<DynamicType = ()>
        + Metadata<Ty = ObjectMeta>
        + DeserializeOwned
        + Serialize
        + Send
        + Sync
        + 'static,
{
    let file_path = Path::new(mount_path)
        .join(cluster_name)
        .join(tf.to_string())
        .join(file_name);

    let file_str = file_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in file path"))?;

    let resource_list: ObjectList<T> = get_resource_list(client).await?;
    utils::write_json_to_file(&resource_list.items, file_str).await
}
