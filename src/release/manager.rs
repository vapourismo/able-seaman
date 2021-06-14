use crate::k8s::transaction;
use crate::k8s::ObjectType;
use crate::k8s::TaggableObject;
use crate::release;
use crate::Objects;
use crate::Release;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;
use kube::Client;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum Error {
    KubeError(kube::Error),
    ReleaseStateError(ReleaseStateError),

    ReleaseError {
        state: ReleaseState,
        error: release::Error,
    },
}

impl From<kube::Error> for Error {
    fn from(error: kube::Error) -> Self {
        Error::KubeError(error)
    }
}

impl From<ReleaseStateError> for Error {
    fn from(error: ReleaseStateError) -> Self {
        Error::ReleaseStateError(error)
    }
}

#[derive(Clone)]
pub struct Manager {
    client: Client,
}

impl Manager {
    pub fn new(client: Client) -> Self {
        Manager { client }
    }

    pub async fn get_release_state(&self, name: &str) -> Result<Option<ReleaseState>, Error> {
        let api: Api<ConfigMap> = Api::default_namespaced(self.client.clone());

        match api.get(name).await {
            Err(kube::Error::Api(kube::error::ErrorResponse {
                reason, code: 404, ..
            })) if reason == "NotFound" => Ok(None),

            Err(err) => Err(Error::KubeError(err)),

            Ok(value) => Ok(Some(ReleaseState::from_config_map(&value)?)),
        }
    }

    pub async fn deploy(&self, release: &Release) -> Result<(), Error> {
        let config_maps: Api<ConfigMap> = Api::default_namespaced(self.client.clone());
        let _lock = release.lock(&config_maps).await?;

        let state = self.get_release_state(release.info.name.as_str()).await?;

        match state {
            None => {
                let state = ReleaseState {
                    current: release.objects.clone(),
                    history: Vec::new(),
                };

                release
                    .install(self.client.clone())
                    .await
                    .map_err(|error| Error::ReleaseError {
                        error,
                        state: state.clone(),
                    })?;

                state
                    .apply(&config_maps, release.info.name.as_str())
                    .await?;
            }

            Some(mut state) => {
                let old_release = Release {
                    info: release.info.clone(),
                    objects: state.current.clone(),
                };

                release
                    .upgrade(&old_release, self.client.clone())
                    .await
                    .map_err(|error| Error::ReleaseError {
                        error,
                        state: state.clone(),
                    })?;

                state.history.insert(0, state.current);
                state.current = release.objects.clone();

                state
                    .apply(&config_maps, release.info.name.as_str())
                    .await?;
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum ReleaseStateError {
    CorruptReleaseState(ConfigMap),
    JSONError(serde_json::Error),
    UpdateError(transaction::Error),
}

impl From<serde_json::Error> for ReleaseStateError {
    fn from(error: serde_json::Error) -> Self {
        ReleaseStateError::JSONError(error)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseState {
    current: Objects,
    history: Vec<Objects>,
}

impl ReleaseState {
    fn from_config_map(config_map: &ConfigMap) -> Result<Self, ReleaseStateError> {
        let release_state = config_map
            .data
            .as_ref()
            .and_then(|data| data.get("release_state"));

        if let Some(data) = release_state {
            Ok(serde_json::from_str(data.as_str())?)
        } else {
            Err(ReleaseStateError::CorruptReleaseState(config_map.clone()))
        }
    }

    fn to_config_map(&self) -> Result<ConfigMap, ReleaseStateError> {
        let mut config_map = ConfigMap::default();
        config_map.tag(ObjectType::ReleaseState);

        config_map
            .data
            .get_or_insert(BTreeMap::new())
            .insert("release_state".to_string(), serde_json::to_string(&self)?);

        Ok(config_map)
    }

    async fn apply(&self, api: &Api<ConfigMap>, name: &str) -> Result<(), ReleaseStateError> {
        let mut config_map = self.to_config_map()?;
        config_map.metadata.name = Some(name.to_string());

        transaction::apply(&api, &config_map)
            .await
            .map_err(|error| ReleaseStateError::UpdateError(error))?;

        Ok(())
    }
}
