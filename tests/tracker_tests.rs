use async_trait::async_trait;
use k8s_openapi::api::core::v1::{Namespace, PersistentVolume, PersistentVolumeClaim};
use k8s_openapi::api::storage::v1::StorageClass;
use kube::api::{ListParams, ObjectList};
use kube::core::ObjectMeta;

#[async_trait]
pub trait KubeClient: Send + Sync {
    async fn list_pv(&self, lp: ListParams) -> Result<ObjectList<PersistentVolume>, anyhow::Error>;
    async fn list_pvc(
        &self,
        lp: ListParams,
    ) -> Result<ObjectList<PersistentVolumeClaim>, anyhow::Error>;
    async fn list_sc(&self, lp: ListParams) -> Result<ObjectList<StorageClass>, anyhow::Error>;
    async fn list_ns(&self, lp: ListParams) -> Result<ObjectList<Namespace>, anyhow::Error>;
}

pub struct MockKubeClient {
    pub pvs: Vec<PersistentVolume>,
    pub pvcs: Vec<PersistentVolumeClaim>,
    pub scs: Vec<StorageClass>,
    pub nss: Vec<Namespace>,
}

#[async_trait]
impl KubeClient for MockKubeClient {
    async fn list_pv(
        &self,
        _lp: ListParams,
    ) -> Result<ObjectList<PersistentVolume>, anyhow::Error> {
        Ok(ObjectList {
            items: self.pvs.clone(),
            metadata: Default::default(),
            types: kube::core::TypeMeta::default(),
        })
    }

    async fn list_pvc(
        &self,
        _lp: ListParams,
    ) -> Result<ObjectList<PersistentVolumeClaim>, anyhow::Error> {
        Ok(ObjectList {
            items: self.pvcs.clone(),
            metadata: Default::default(),
            types: kube::core::TypeMeta::default(),
        })
    }

    async fn list_sc(&self, _lp: ListParams) -> Result<ObjectList<StorageClass>, anyhow::Error> {
        Ok(ObjectList {
            items: self.scs.clone(),
            metadata: Default::default(),
            types: kube::core::TypeMeta::default(),
        })
    }

    async fn list_ns(&self, _lp: ListParams) -> Result<ObjectList<Namespace>, anyhow::Error> {
        Ok(ObjectList {
            items: self.nss.clone(),
            metadata: Default::default(),
            types: kube::core::TypeMeta::default(),
        })
    }
}

// --- Logic functions using the trait (could be in your lib.rs)
pub async fn get_pv_from<C: KubeClient>(
    client: &C,
) -> Result<ObjectList<PersistentVolume>, anyhow::Error> {
    client.list_pv(ListParams::default()).await
}

pub async fn get_pvc_from<C: KubeClient>(
    client: &C,
) -> Result<ObjectList<PersistentVolumeClaim>, anyhow::Error> {
    client.list_pvc(ListParams::default()).await
}

pub async fn get_sc_from<C: KubeClient>(
    client: &C,
) -> Result<ObjectList<StorageClass>, anyhow::Error> {
    client.list_sc(ListParams::default()).await
}

pub async fn get_ns_from<C: KubeClient>(
    client: &C,
) -> Result<ObjectList<Namespace>, anyhow::Error> {
    client.list_ns(ListParams::default()).await
}

// --- Test helpers
fn make_pv(name: &str) -> PersistentVolume {
    PersistentVolume {
        metadata: ObjectMeta {
            name: Some(name.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn make_pvc(name: &str) -> PersistentVolumeClaim {
    PersistentVolumeClaim {
        metadata: ObjectMeta {
            name: Some(name.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn make_sc(name: &str) -> StorageClass {
    StorageClass {
        metadata: ObjectMeta {
            name: Some(name.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn make_ns(name: &str) -> Namespace {
    Namespace {
        metadata: ObjectMeta {
            name: Some(name.into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

// --- Unit Tests
#[tokio::test]
async fn test_get_pv_from_mock() {
    let mock = MockKubeClient {
        pvs: vec![make_pv("pv-test")],
        pvcs: vec![],
        scs: vec![],
        nss: vec![],
    };

    let result = get_pv_from(&mock).await.unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].metadata.name.as_deref(), Some("pv-test"));
}

#[tokio::test]
async fn test_get_pvc_from_mock() {
    let mock = MockKubeClient {
        pvs: vec![],
        pvcs: vec![make_pvc("pvc-test")],
        scs: vec![],
        nss: vec![],
    };

    let result = get_pvc_from(&mock).await.unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].metadata.name.as_deref(), Some("pvc-test"));
}

#[tokio::test]
async fn test_get_sc_from_mock() {
    let mock = MockKubeClient {
        pvs: vec![],
        pvcs: vec![],
        scs: vec![make_sc("sc-test")],
        nss: vec![],
    };

    let result = get_sc_from(&mock).await.unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].metadata.name.as_deref(), Some("sc-test"));
}

#[tokio::test]
async fn test_get_ns_from_mock() {
    let mock = MockKubeClient {
        pvs: vec![],
        pvcs: vec![],
        scs: vec![],
        nss: vec![make_ns("ns-test")],
    };

    let result = get_ns_from(&mock).await.unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].metadata.name.as_deref(), Some("ns-test"));
}
