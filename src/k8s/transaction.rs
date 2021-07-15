use crate::meta::CRATE_NAME;
use crate::objects::Object;
use kube::api;
use kube::core::DynamicObject;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error;
use std::fmt;

#[derive(Debug)]
pub enum Action {
    Create,
    Apply,
    Delete,
}

impl fmt::Display for Action {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(formatter, "{:?}", self)
    }
}

pub struct EndResult {
    pub client: kube::Client,
    pub result_object: DynamicObject,
}

#[derive(Debug)]
pub enum Error {
    NeedApiResource {
        object: DynamicObject,
    },

    NeedName {
        object_rep: String,
    },

    KubeError {
        kube_error: kube::Error,
        action: Action,
        object_name: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Error::NeedApiResource { object } => {
                write!(
                    formatter,
                    "Cannot determine API resource type for: {:?}",
                    object
                )
            }

            Error::NeedName { object_rep } => {
                write!(formatter, "Resource needs a name: {}", object_rep)
            }

            Error::KubeError {
                kube_error,
                action,
                object_name,
            } => write!(
                formatter,
                "Kubernetes error while trying to {} {}: {}",
                action, object_name, kube_error
            ),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::KubeError { kube_error, .. } => Some(kube_error),
            _ => None,
        }
    }
}

pub async fn apply<SomeResource>(
    api: &kube::Api<SomeResource>,
    object: &SomeResource,
) -> Result<SomeResource, Error>
where
    SomeResource: kube::ResourceExt + Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    let name = object.meta().name.as_ref().ok_or(Error::NeedName {
        object_rep: format!("{:?}", object),
    })?;

    let patched = api
        .patch(
            name.as_str(),
            &api::PatchParams::apply(CRATE_NAME).force(),
            &api::Patch::Apply(object.clone()),
        )
        .await
        .map_err(|kube_error| Error::KubeError {
            kube_error,
            action: Action::Apply,
            object_name: name.clone(),
        })?;

    Ok(patched)
}

pub async fn apply_object(client: kube::Client, object: &Object) -> Result<EndResult, Error> {
    let api = kube::Api::default_namespaced_with(client, &object.api_resource);

    let patched = apply(&api, &object.dyn_object).await?;

    Ok(EndResult {
        client: api.into_client(),
        result_object: patched,
    })
}

pub async fn create<SomeResource>(
    api: &kube::Api<SomeResource>,
    object: &SomeResource,
) -> Result<SomeResource, Error>
where
    SomeResource: kube::Resource + Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    let name = object.meta().name.as_ref().ok_or(Error::NeedName {
        object_rep: format!("{:?}", object),
    })?;

    let result = api
        .create(&api::PostParams::default(), object)
        .await
        .map_err(|kube_error| Error::KubeError {
            kube_error,
            action: Action::Create,
            object_name: name.clone(),
        })?;

    Ok(result)
}

pub async fn create_object(client: kube::Client, object: &Object) -> Result<EndResult, Error> {
    let api = kube::Api::default_namespaced_with(client, &object.api_resource);

    let result = create(&api, &object.dyn_object).await?;

    Ok(EndResult {
        client: api.into_client(),
        result_object: result,
    })
}

pub async fn delete<SomeResource>(
    api: &kube::Api<SomeResource>,
    object: &SomeResource,
) -> Result<(), Error>
where
    SomeResource: kube::ResourceExt + Clone + fmt::Debug + DeserializeOwned,
{
    let name = object.meta().name.as_ref().ok_or(Error::NeedName {
        object_rep: format!("{:?}", object),
    })?;

    api.delete(name, &api::DeleteParams::default())
        .await
        .map_err(|kube_error| Error::KubeError {
            kube_error,
            action: Action::Delete,
            object_name: name.clone(),
        })?;

    Ok(())
}

pub async fn delete_object(client: kube::Client, object: &Object) -> Result<kube::Client, Error> {
    let api = kube::Api::default_namespaced_with(client, &object.api_resource);

    delete(&api, &object.dyn_object).await?;

    Ok(api.into_client())
}
