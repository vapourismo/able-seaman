use futures::StreamExt;
use futures::TryStreamExt;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::DeleteParams;
use kube::api::ListParams;
use kube::api::PostParams;
use kube::api::WatchEvent;
use kube::core::ApiResource;
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::core::TypeMeta;
use kube::Api;
use kube::ResourceExt;
use std::collections::BTreeMap;

fn split_api_version(api_version: &str) -> (&str, &str) {
    if let Some((group, version)) = api_version.split_once('/') {
        (group, version)
    } else {
        ("", api_version)
    }
}

pub trait ToApiResource {
    fn to_resource(&self) -> ApiResource;
}

impl ToApiResource for ApiResource {
    fn to_resource(&self) -> ApiResource {
        self.clone()
    }
}

impl ToApiResource for TypeMeta {
    fn to_resource(&self) -> ApiResource {
        let (group, version) = split_api_version(self.api_version.as_str());
        ApiResource::from_gvk(&GroupVersionKind::gvk(group, version, self.kind.as_str()))
    }
}

pub trait TryToApiResource {
    fn try_to_api_resource(&self) -> Option<ApiResource>;
}

impl TryToApiResource for ApiResource {
    fn try_to_api_resource(&self) -> Option<ApiResource> {
        Some(self.clone())
    }
}

impl TryToApiResource for DynamicObject {
    fn try_to_api_resource(&self) -> Option<ApiResource> {
        self.types.as_ref().map(|types| types.to_resource())
    }
}

async fn wait_for_deletion(api: &Api<ConfigMap>, name: &String) -> Result<(), kube::Error> {
    let mut stream = api
        .watch(
            &ListParams::default()
                .labels("able-seaman/function=lock")
                .timeout(10),
            "0",
        )
        .await?
        .boxed();

    while let Some(event) = stream.try_next().await? {
        match event {
            WatchEvent::Deleted(deletion) if &deletion.name() == name => {
                return Ok(());
            }

            _ => {}
        }
    }

    Ok(())
}

pub struct Lock<'a> {
    api: &'a Api<ConfigMap>,
    name: String,
}

impl<'a> Lock<'a> {
    pub async fn new(api: &'a Api<ConfigMap>, name: String) -> Result<Lock<'a>, kube::Error> {
        let mut lock_config = ConfigMap::default();
        lock_config.metadata.name = Some(name.clone());

        let mut labels = BTreeMap::new();
        labels.insert("able-seaman/function".to_string(), "lock".to_string());
        lock_config.metadata.labels = Some(labels);

        loop {
            match api.create(&PostParams::default(), &lock_config).await {
                Err(kube::Error::Api(kube::error::ErrorResponse {
                    reason, code: 409, ..
                })) if reason == "AlreadyExists" => {
                    wait_for_deletion(&api, &name).await?;
                }

                result => {
                    result?;
                    break;
                }
            }
        }

        Ok(Lock { api, name })
    }
}

impl<'a> Drop for Lock<'a> {
    fn drop(&mut self) {
        let deletion = futures::executor::block_on(
            self.api
                .delete(self.name.as_str(), &DeleteParams::default()),
        );

        match deletion {
            Err(err) => {
                eprintln!("Failed to delete locking ConfigMap {}: {}", self.name, err);
            }

            _ => {}
        }
    }
}
