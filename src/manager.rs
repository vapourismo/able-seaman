use crate::k8s;
use crate::k8s::annotations::WithAnnotations;
use crate::k8s::api_resource;
use crate::k8s::labels;
use crate::k8s::labels::WithLabels;
use crate::k8s::transaction;
use crate::release;
use crate::release::plan;
use k8s_openapi::api::core::v1::ConfigMap;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::str;

#[derive(Debug)]
pub enum Error {
    KubeError(kube::Error),
    ReleaseStateError(Box<ReleaseStateError>),

    ReleaseError {
        state: ReleaseState,
        error: Box<release::Error>,
    },
}

impl From<kube::Error> for Error {
    fn from(error: kube::Error) -> Self {
        Error::KubeError(error)
    }
}

impl From<ReleaseStateError> for Error {
    fn from(error: ReleaseStateError) -> Self {
        Error::ReleaseStateError(Box::new(error))
    }
}

#[derive(Clone, Debug)]
pub enum DeployResult {
    Unchanged,
    Installed { plan: plan::ReleasePlan },
    Upgraded { plan: plan::ReleasePlan },
}

#[derive(Clone, Debug)]
pub enum NamespaceMode {
    Default,
    Specific(String),
}

impl NamespaceMode {
    pub fn new(namespace: Option<String>) -> NamespaceMode {
        namespace
            .map(NamespaceMode::Specific)
            .unwrap_or(NamespaceMode::Default)
    }
}

#[derive(Clone)]
pub struct Manager {
    client: kube::Client,
    config_maps: kube::Api<ConfigMap>,
}

impl Manager {
    pub async fn new(ns_mode: NamespaceMode) -> Result<Self, Error> {
        let mut config = kube::Config::infer().await?;
        match ns_mode {
            NamespaceMode::Default => {}
            NamespaceMode::Specific(ns) => {
                config.default_namespace = ns;
            }
        }

        let client = kube::Client::try_from(config)?;
        let config_maps = kube::Api::default_namespaced(client.clone());

        Ok(Manager {
            client,
            config_maps,
        })
    }

    pub async fn deploy(&self, release: &release::Release) -> Result<DeployResult, Error> {
        let name = release.name();
        let lock = release.lock(&self.config_maps).await?;
        let state = ReleaseState::get(&self.config_maps, name.as_str()).await?;

        let result = match state {
            None => {
                let state = ReleaseState {
                    current: release.objects().clone(),
                    history: Vec::new(),
                };

                let (_client, plan) =
                    release
                        .install(self.client.clone())
                        .await
                        .map_err(|error| Error::ReleaseError {
                            error: Box::new(error),
                            state: state.clone(),
                        })?;

                state.apply(&self.config_maps, name.as_str()).await?;

                DeployResult::Installed { plan }
            }

            Some(mut state) => {
                let old_release =
                    release::Release::from_objects(name.clone(), state.current.clone());

                if old_release.hash_value() == release.hash_value() {
                    return Ok(DeployResult::Unchanged);
                }

                let (_client, plan) = release
                    .upgrade(&old_release, self.client.clone())
                    .await
                    .map_err(|error| Error::ReleaseError {
                        error: Box::new(error),
                        state: state.clone(),
                    })?;

                state.history.insert(0, state.current);
                state.current = release.objects().clone();

                state.apply(&self.config_maps, name.as_str()).await?;

                DeployResult::Upgraded { plan }
            }
        };

        lock.release().await?;
        Ok(result)
    }

    pub async fn delete(&self, name: String) -> Result<Option<plan::ReleasePlan>, Error> {
        let state = ReleaseState::get(&self.config_maps, name.as_str()).await?;

        if let Some(state) = state {
            let release = release::Release::from_objects(name, state.current.clone());

            let (client, plan) = release
                .uninstall(self.client.clone())
                .await
                .map_err(|error| Error::ReleaseError {
                    error: Box::new(error),
                    state,
                })?;

            let api: kube::Api<ConfigMap> = kube::Api::default_namespaced(client);

            api.delete(release.name(), &kube::api::DeleteParams::default())
                .await?;

            Ok(Some(plan))
        } else {
            Ok(None)
        }
    }

    pub async fn verify(&self, _name: String) -> Result<(), kube::Error> {
        let mut client = self.client.clone();
        let all_resources = api_resource::find_api_resources(&client).await?;

        for resource in all_resources {
            let api: kube::Api<kube::core::DynamicObject> = kube::Api::all_with(client, &resource);

            let items = api
                .list(&labels::Labels::from(k8s::ObjectType::Managed).to_listparams())
                .await?
                .items;

            items.iter().for_each(|item| {
                dbg!(item);
            });

            client = api.into_client();
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum ReleaseStateError {
    CorruptReleaseState(ConfigMap),
    JSONError(serde_json::Error),
    UpdateError(transaction::Error),
    KubeError(kube::Error),
}

impl From<serde_json::Error> for ReleaseStateError {
    fn from(error: serde_json::Error) -> Self {
        ReleaseStateError::JSONError(error)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseState {
    current: release::Objects,
    history: Vec<release::Objects>,
}

impl ReleaseState {
    fn from_config_map(config_map: &ConfigMap) -> Result<Self, ReleaseStateError> {
        let release_state = config_map.data.get("release_state");

        if let Some(data) = release_state {
            Ok(serde_json::from_str(data.as_str())?)
        } else {
            Err(ReleaseStateError::CorruptReleaseState(config_map.clone()))
        }
    }

    fn to_config_map(&self) -> Result<ConfigMap, ReleaseStateError> {
        let mut config_map = ConfigMap::default()
            .with_label(&k8s::ObjectType::ReleaseState)
            .with_annotation(&k8s::CrateVersion);

        config_map
            .data
            .insert("release_state".to_string(), serde_json::to_string(&self)?);

        Ok(config_map)
    }

    pub async fn get(
        api: &kube::Api<ConfigMap>,
        name: &str,
    ) -> Result<Option<Self>, ReleaseStateError> {
        match api.get(name).await {
            Err(kube::Error::Api(kube::error::ErrorResponse {
                reason, code: 404, ..
            })) if reason == "NotFound" => Ok(None),

            Err(err) => Err(ReleaseStateError::KubeError(err)),

            Ok(value) => Ok(Some(ReleaseState::from_config_map(&value)?)),
        }
    }

    async fn apply(&self, api: &kube::Api<ConfigMap>, name: &str) -> Result<(), ReleaseStateError> {
        let mut config_map = self.to_config_map()?;
        config_map.metadata.name = Some(name.to_string());

        transaction::apply(&api, &config_map)
            .await
            .map_err(ReleaseStateError::UpdateError)?;

        Ok(())
    }
}
