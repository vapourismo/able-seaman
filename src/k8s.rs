use crate::meta::CRATE_VERSION;
use futures::StreamExt;
use futures::TryStreamExt;
use kube::api::DeleteParams;
use kube::api::ListParams;
use kube::api::PostParams;
use kube::api::WatchEvent;
use kube::core::ApiResource;
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::core::TypeMeta;
use kube::Api;
use kube::Resource;
use kube::ResourceExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

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

const TYPE_LABEL: &'static str = "able-seaman/type";
const VERSION_LABEL: &'static str = "able-seaman/version";

fn tag_object<SomeResource>(object: &mut SomeResource, object_type: String)
where
    SomeResource: ResourceExt,
{
    object
        .labels_mut()
        .insert(TYPE_LABEL.to_string(), object_type);

    object
        .annotations_mut()
        .insert(VERSION_LABEL.to_string(), CRATE_VERSION.to_string());
}

async fn wait_for_deletion<SomeResource>(
    api: &Api<SomeResource>,
    name: &String,
) -> Result<(), kube::Error>
where
    SomeResource: Clone + DeserializeOwned + Debug + ResourceExt,
{
    let mut stream = api
        .watch(
            &ListParams::default()
                .labels(format!("{}=lock", TYPE_LABEL).as_str())
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

pub struct Lock<'a, T>
where
    T: Clone + DeserializeOwned + Debug,
{
    api: &'a Api<T>,
    name: String,
}

impl<'a, T> Lock<'a, T>
where
    T: Resource + Default + Clone + Debug + DeserializeOwned + Serialize,
{
    pub async fn new(api: &'a Api<T>, name: String) -> Result<Lock<'a, T>, kube::Error> {
        Lock::new_with(api, name, <T as Default>::default()).await
    }

    pub async fn new_with(
        api: &'a Api<T>,
        name: String,
        mut lock_value: T,
    ) -> Result<Lock<'a, T>, kube::Error> {
        lock_value.meta_mut().name = Some(name.clone());
        tag_object(&mut lock_value, "lock".to_string());

        let _locked_value = loop {
            match api.create(&PostParams::default(), &lock_value).await {
                Err(kube::Error::Api(kube::error::ErrorResponse {
                    reason, code: 409, ..
                })) if reason == "AlreadyExists" => {
                    wait_for_deletion(&api, &name).await?;
                }

                result => {
                    break result?;
                }
            }
        };

        Ok(Lock { api, name })
    }
}

impl<'a, T> Drop for Lock<'a, T>
where
    T: Clone + DeserializeOwned + Debug,
{
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
