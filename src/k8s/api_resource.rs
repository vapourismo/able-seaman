use kube::core::ApiResource;
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::core::TypeMeta;
use std::collections::HashSet;

fn split_api_version(api_version: &str) -> (&str, &str) {
    if let Some((group, version)) = api_version.split_once('/') {
        (group, version)
    } else {
        ("", api_version)
    }
}

pub trait ToApiResource {
    fn to_api_resource(&self) -> ApiResource;
}

impl ToApiResource for ApiResource {
    fn to_api_resource(&self) -> ApiResource {
        self.clone()
    }
}

impl ToApiResource for TypeMeta {
    fn to_api_resource(&self) -> ApiResource {
        let (group, version) = split_api_version(self.api_version.as_str());
        ApiResource::from_gvk(&GroupVersionKind::gvk(group, version, self.kind.as_str()))
    }
}

pub trait TryToApiResource {
    fn try_to_api_resource(&self) -> Option<ApiResource>;
}

impl TryToApiResource for ApiResource {
    fn try_to_api_resource(&self) -> Option<ApiResource> {
        Some(self.to_api_resource())
    }
}

impl TryToApiResource for TypeMeta {
    fn try_to_api_resource(&self) -> Option<ApiResource> {
        Some(self.to_api_resource())
    }
}

impl TryToApiResource for DynamicObject {
    fn try_to_api_resource(&self) -> Option<ApiResource> {
        self.types.as_ref().map(|types| types.to_api_resource())
    }
}

struct ApiResourceWrapper(ApiResource);

impl PartialEq<ApiResourceWrapper> for ApiResourceWrapper {
    fn eq(&self, rhs: &Self) -> bool {
        self.0 == rhs.0
    }
}

impl Eq for ApiResourceWrapper {}

pub async fn find_api_resources(
    client: &kube::Client,
) -> Result<HashSet<ApiResource>, kube::Error> {
    let mut resources = HashSet::new();

    for core_version in client.list_core_api_versions().await?.versions {
        let core_resources = client
            .list_core_api_resources(core_version.as_str())
            .await?;

        for core_resource in core_resources.resources {
            if !(core_resource.verbs.contains(&"get".to_string())
                && core_resource.verbs.contains(&"list".to_string()))
            {
                continue;
            }

            let resource = ApiResource::from_gvk_with_plural(
                &GroupVersionKind::gvk(
                    "",
                    core_resource
                        .version
                        .as_ref()
                        .unwrap_or(&core_version.clone())
                        .as_ref(),
                    core_resource.kind.as_ref(),
                ),
                core_resource.name.as_ref(),
            );

            resources.insert(resource);
        }
    }

    for group in client.list_api_groups().await?.groups {
        for version in group.versions {
            let group_resources = client
                .list_api_group_resources(version.group_version.as_str())
                .await?;

            for group_resource in group_resources.resources {
                if !(group_resource.verbs.contains(&"get".to_string())
                    && group_resource.verbs.contains(&"list".to_string()))
                {
                    continue;
                }

                let resource = ApiResource::from_gvk_with_plural(
                    &GroupVersionKind::gvk(
                        group_resource
                            .group
                            .as_ref()
                            .unwrap_or(&group.name)
                            .as_ref(),
                        group_resource
                            .version
                            .as_ref()
                            .unwrap_or(&version.version)
                            .as_ref(),
                        group_resource.kind.as_ref(),
                    ),
                    group_resource.name.as_ref(),
                );

                resources.insert(resource);
            }
        }
    }

    Ok(resources)
}
