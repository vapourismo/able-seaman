mod errors;
mod k8s;
mod meta;
mod objects;
mod release;
mod resources;

use crate::errors::GeneralError;
use crate::k8s::TryToApiResource;
use crate::release::Objects;
use crate::release::Release;
use crate::release::ReleaseInfo;
use clap::Clap;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::Patch;
use kube::api::PatchParams;
use kube::core::DynamicObject;
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

async fn apply_all(client: Client, objects: &Objects) -> Result<(), GeneralError> {
    let mut client = client;

    for (name, object) in objects {
        if let Some(api_resource) = object.try_to_api_resource() {
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

            {
                let _lock = release.lock(&config_maps).await?;

                match config_maps.get(release.info.name.as_str()).await {
                    Err(kube::Error::Api(ErrorResponse { reason, code, .. }))
                        if reason == "NotFound" && code == 404 =>
                    {
                        apply_all(client, &release.objects).await?;
                    }

                    Ok(_existing_config) => {
                        apply_all(client, &release.objects).await?;
                    }

                    result => {
                        result?;
                    }
                };
            }

            let mut release_config = release.to_config_map()?;
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
