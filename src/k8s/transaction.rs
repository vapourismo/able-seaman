use crate::k8s::api_resource::TryToApiResource;
use crate::meta::CRATE_NAME;
use kube::api::DeleteParams;
use kube::api::Patch;
use kube::api::PatchParams;
use kube::api::PostParams;
use kube::core::DynamicObject;
use kube::Api;
use kube::Client;
use kube::ResourceExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

pub struct EndResult {
    pub client: Client,
    pub result_object: DynamicObject,
}

#[derive(Debug)]
pub enum DynamicError {
    NeedApiResource,
    NeedName,
    KubeError(kube::Error),
}

impl From<kube::Error> for DynamicError {
    fn from(error: kube::Error) -> Self {
        DynamicError::KubeError(error)
    }
}

pub async fn apply<SomeResource>(
    api: &Api<SomeResource>,
    object: &SomeResource,
) -> Result<SomeResource, DynamicError>
where
    SomeResource: ResourceExt + Clone + Debug + Serialize + DeserializeOwned,
{
    let name = object.meta().name.as_ref().ok_or(DynamicError::NeedName)?;

    let patched = api
        .patch(
            name.as_str(),
            &PatchParams::apply(CRATE_NAME),
            &Patch::Apply(object.clone()),
        )
        .await?;

    Ok(patched)
}

pub async fn apply_dynamic(
    client: Client,
    object: &DynamicObject,
) -> Result<EndResult, DynamicError> {
    let api_resource = object
        .try_to_api_resource()
        .ok_or(DynamicError::NeedApiResource)?;
    let api = Api::default_namespaced_with(client, &api_resource);

    let patched = apply(&api, object).await?;

    Ok(EndResult {
        client: api.into_client(),
        result_object: patched,
    })
}

pub async fn create<SomeResource>(
    api: &Api<SomeResource>,
    object: &SomeResource,
) -> Result<SomeResource, DynamicError>
where
    SomeResource: Clone + Debug + Serialize + DeserializeOwned,
{
    let result = api.create(&PostParams::default(), object).await?;
    Ok(result)
}

pub async fn create_dynamic(
    client: Client,
    object: &DynamicObject,
) -> Result<EndResult, DynamicError> {
    let api_resource = object
        .try_to_api_resource()
        .ok_or(DynamicError::NeedApiResource)?;
    let api = Api::default_namespaced_with(client, &api_resource);

    let result = create(&api, object).await?;

    Ok(EndResult {
        client: api.into_client(),
        result_object: result,
    })
}

pub async fn delete<SomeResource>(
    api: &Api<SomeResource>,
    object: &SomeResource,
) -> Result<(), DynamicError>
where
    SomeResource: ResourceExt + Clone + Debug + DeserializeOwned,
{
    let name = object.meta().name.as_ref().ok_or(DynamicError::NeedName)?;

    api.delete(name, &DeleteParams::default()).await?;

    Ok(())
}

pub async fn delete_dynamic(
    client: Client,
    object: &DynamicObject,
) -> Result<Client, DynamicError> {
    let api_resource = object
        .try_to_api_resource()
        .ok_or(DynamicError::NeedApiResource)?;
    let api = Api::default_namespaced_with(client, &api_resource);

    delete(&api, object).await?;

    Ok(api.into_client())
}
