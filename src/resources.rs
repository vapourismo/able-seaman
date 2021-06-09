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

pub struct ApiKnowledge {
    api_resources: Vec<ApiResource>,
}

impl ApiKnowledge {
    pub async fn new(client: &Client) -> Result<Self, GeneralError> {
        let mut api_resources = get_core_api_resources(&client).await?;
        api_resources.append(&mut get_api_resources(&client).await?);

        Ok(ApiKnowledge { api_resources })
    }
}

#[derive(Clone, Debug)]
pub struct ReleasedObject {
    pub name: String,
    pub object: DynamicObject,
}

#[derive(Clone, Debug)]
pub struct ReleasedApi {
    pub api_resource: ApiResource,
    pub objects: Vec<ReleasedObject>,
}

pub async fn list_release_resources(
    client: Client,
    knowledge: &ApiKnowledge,
    release_info: &ReleaseInfo,
) -> Result<(Client, Vec<ReleasedApi>), GeneralError> {
    let mut all_apis = Vec::new();
    let mut client = client;

    for api_resource in &knowledge.api_resources {
        let api: Api<DynamicObject> = Api::all_with(client, api_resource);

        let objects = api.list(&release_info.to_list_params()).await?;
        let mut released_objects = Vec::new();

        for object in objects {
            if let Some(name) = &object.metadata.name {
                released_objects.push(ReleasedObject {
                    name: name.clone(),
                    object,
                });
            }
        }

        if released_objects.len() > 0 {
            all_apis.push(ReleasedApi {
                api_resource: api_resource.clone(),
                objects: released_objects,
            });
        }

        client = api.into_client();
    }

    Ok((client, all_apis))
}
