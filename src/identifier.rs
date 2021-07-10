use crate::k8s::api_resource::TryToApiResource;
use kube::core::GroupVersionKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Identifier {
    gvk: GroupVersionKind,
    name: String,
}

impl Identifier {
    pub fn from_resource<R>(object: &R) -> Option<Self>
    where
        R: kube::Resource + TryToApiResource,
    {
        let name = object.meta().name.clone()?;
        let api_resource = object.try_to_api_resource()?;
        let gvk = kube::core::GroupVersionKind {
            group: api_resource.group,
            kind: api_resource.kind,
            version: api_resource.version,
        };

        Some(Identifier { gvk, name })
    }

    pub fn from_api_resource(name: String, api_resource: &kube::core::ApiResource) -> Option<Self> {
        let gvk = kube::core::GroupVersionKind {
            group: api_resource.group.clone(),
            kind: api_resource.kind.clone(),
            version: api_resource.version.clone(),
        };

        Some(Identifier { gvk, name })
    }
}
