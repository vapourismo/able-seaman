mod errors;
mod objects;
mod release;
mod resources;

use crate::errors::GeneralError;
use crate::release::Objects;
use crate::release::Release;
use crate::release::ReleaseInfo;
use clap::Clap;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::DeleteParams;
use kube::api::Patch;
use kube::api::PatchParams;
use kube::api::PostParams;
use kube::core::ApiResource;
use kube::core::DynamicObject;
use kube::core::GroupVersionKind;
use kube::core::TypeMeta;
use kube::error::ErrorResponse;
use kube::Api;
use kube::Client;
use std::path::Path;

#[derive(Clap, Clone, Debug)]
enum Command {
    Package {
        release_name: String,
        input_files: Vec<String>,
    },
    Deploy {
        release_name: String,
        input_files: Vec<String>,
    },
}

#[derive(Clap, Clone, Debug)]
struct Options {
    #[clap(subcommand)]
    command: Command,
}

fn object_to_api_resource(typ: &TypeMeta) -> ApiResource {
    ApiResource::from_gvk(
        &if let Some((group, version)) = typ.api_version.as_str().split_once('/') {
            GroupVersionKind::gvk(group, version, typ.kind.as_str())
        } else {
            GroupVersionKind::gvk("", typ.api_version.as_str(), typ.kind.as_str())
        },
    )
}

async fn apply_all(client: Client, objects: &Objects) -> Result<(), GeneralError> {
    let mut client = client;

    for (name, object) in objects {
        if let Some(ref typ) = object.types {
            let api_resource = object_to_api_resource(typ);
            let object_api: Api<DynamicObject> =
                Api::default_namespaced_with(client, &api_resource);

            object_api
                .patch(
                    name.as_str(),
                    &PatchParams::apply("able-seaman").force(),
                    &Patch::Apply(object),
                )
                .await?;

            client = object_api.into_client();
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), GeneralError> {
    let options = Options::parse();

    match options.command {
        Command::Package {
            release_name,
            input_files,
        } => {
            let mut release = Release::new(ReleaseInfo { name: release_name });

            for ref file in input_files {
                release.ingest_objects_from_path(Path::new(file))?;
            }

            println!("{}", serde_json::to_string_pretty(&release)?);
        }

        Command::Deploy {
            release_name,
            input_files,
        } => {
            let mut release = Release::new(ReleaseInfo { name: release_name });

            for ref file in input_files {
                release.ingest_objects_from_path(Path::new(file))?;
            }

            let client = Client::try_default().await?;
            let config_maps: Api<ConfigMap> = Api::default_namespaced(client.clone());

            let lock_name = format!("{}-lock", release.info.name);
            let mut lock_config = release.as_config_map()?;
            lock_config.metadata.name = Some(lock_name.clone());

            let create_result = config_maps
                .create(&PostParams::default(), &lock_config)
                .await;

            match create_result {
                Err(kube::Error::Api(ErrorResponse { reason, code, .. }))
                    if reason == "AlreadyExists" && code == 409 =>
                {
                    return Err(GeneralError::ReleaseIsBusy);
                }
                _ => {
                    create_result?;
                }
            }

            let result = match config_maps.get(release.info.name.as_str()).await {
                Err(kube::Error::Api(ErrorResponse { reason, code, .. }))
                    if reason == "NotFound" && code == 404 =>
                {
                    apply_all(client, &release.objects).await
                }

                Ok(_existing_config) => apply_all(client, &release.objects).await,

                result => result
                    .map(|_| ())
                    .map_err(|err| GeneralError::KubeError(err)),
            };

            config_maps
                .delete(lock_name.as_str(), &DeleteParams::default())
                .await?;

            result?;

            let mut release_config = lock_config.clone();
            release_config.metadata.name = Some(release.info.name.clone());

            config_maps
                .patch(
                    release.info.name.as_str(),
                    &PatchParams::apply("able-seaman").force(),
                    &Patch::Apply(release_config),
                )
                .await?;
        }
    }

    Ok(())
}
