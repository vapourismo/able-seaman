use crate::errors::GeneralError;
use crate::release::ReleaseInfo;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;
use kube::core::ApiResource;
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::Api;
use kube::Client;

fn is_eligible_api_resource(resource: &APIResource) -> bool {
    let can_create = resource.verbs.contains(&"create".to_string());
    let can_list = resource.verbs.contains(&"list".to_string());
    let can_get = resource.verbs.contains(&"get".to_string());
    let can_patch = resource.verbs.contains(&"patch".to_string());
    let can_delete = resource.verbs.contains(&"delete".to_string());

    return can_create && can_list && can_get && can_patch && can_delete;
}

fn to_api_resource(group: &String, core_version: &String, resource: &APIResource) -> ApiResource {
    ApiResource::from_gvk_with_plural(
        &GroupVersionKind::gvk(
            resource.group.as_ref().unwrap_or(&group.clone()).as_ref(),
            resource
                .version
                .as_ref()
                .unwrap_or(&core_version.clone())
                .as_ref(),
            resource.kind.as_ref(),
        ),
        resource.name.as_ref(),
    )
}

pub async fn get_core_api_resources(client: &Client) -> Result<Vec<ApiResource>, GeneralError> {
    let mut all_resources = Vec::new();

    let core_versions = client.list_core_api_versions().await?;

    for core_version in core_versions.versions {
        let core_resources = client
            .list_core_api_resources(core_version.as_ref())
            .await?;

        for api_resource in core_resources.resources {
            if !is_eligible_api_resource(&api_resource) {
                continue;
            }

            let resource = to_api_resource(&"".to_string(), &core_version, &api_resource);
            all_resources.push(resource);
        }
    }

    Ok(all_resources)
}

pub async fn get_api_resources(client: &Client) -> Result<Vec<ApiResource>, GeneralError> {
    let mut all_resources = Vec::new();

    let groups = client.list_api_groups().await?;
    for group in groups.groups {
        let try_versions = if let Some(ideal_version) = group.preferred_version {
            vec![ideal_version]
        } else {
            group.versions
        };

        for version in try_versions {
            let group_resources = client
                .list_api_group_resources(version.group_version.as_ref())
                .await?;

            for group_resource in group_resources.resources {
                if !is_eligible_api_resource(&group_resource) {
                    continue;
                }

                let resource = to_api_resource(&group.name, &version.version, &group_resource);
                all_resources.push(resource);
            }
        }
    }

    Ok(all_resources)
}

pub async fn list_release_resources(
    client: &Client,
    api_resource: &ApiResource,
    release_info: &ReleaseInfo,
) -> Result<Vec<DynamicObject>, GeneralError> {
    let api: Api<DynamicObject> = Api::all_with(client.clone(), api_resource);
    let objects = api.list(&release_info.to_list_params()).await?;

    Ok(objects.items)
}
