use kube::core::ApiResource;
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::core::TypeMeta;

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
