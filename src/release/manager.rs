use crate::k8s::apply;
use crate::k8s::ObjectType;
use crate::k8s::TaggableObject;
use crate::release::DynamicError;
use crate::release::ReleaseError;
use crate::Objects;
use crate::Release;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Api;
use kube::Client;
use kube::ResourceExt;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum ManagerError {
    KubeError(kube::Error),
    ReleaseStateError(ReleaseStateError),
    DynamicError(DynamicError),
    ReleaseError(ReleaseError),
}

impl From<kube::Error> for ManagerError {
    fn from(error: kube::Error) -> Self {
        ManagerError::KubeError(error)
    }
}

impl From<ReleaseStateError> for ManagerError {
    fn from(error: ReleaseStateError) -> Self {
        ManagerError::ReleaseStateError(error)
    }
}

impl From<DynamicError> for ManagerError {
    fn from(error: DynamicError) -> Self {
        ManagerError::DynamicError(error)
    }
}

impl From<ReleaseError> for ManagerError {
    fn from(error: ReleaseError) -> Self {
        ManagerError::ReleaseError(error)
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

    pub async fn get_release_state(
        &self,
        name: &str,
    ) -> Result<Option<ReleaseState>, ManagerError> {
        let api: Api<ConfigMap> = Api::default_namespaced(self.client.clone());

        match api.get(name).await {
            Err(kube::Error::Api(kube::error::ErrorResponse {
                reason, code: 404, ..
            })) if reason == "NotFound" => Ok(None),

            Err(err) => Err(ManagerError::KubeError(err)),

            Ok(value) => Ok(Some(ReleaseState::from_config_map(&value)?)),
        }
    }

    pub async fn put_release_state(
        &self,
        name: &str,
        release_state: &ReleaseState,
    ) -> Result<(), ManagerError> {
        let api: Api<ConfigMap> = Api::default_namespaced(self.client.clone());

        let mut config_map = release_state.to_config_map()?;
        config_map.metadata.name = Some(name.to_string());

        apply(&api, &config_map).await?;

        Ok(())
    }

    pub async fn deploy(&self, release: &Release) -> Result<(), ManagerError> {
        let config_maps: Api<ConfigMap> = Api::default_namespaced(self.client.clone());
        let _lock = release.lock(&config_maps).await?;

        let state = self.get_release_state(release.info.name.as_str()).await?;

        match state {
            None => {
                let state = ReleaseState {
                    current: release.objects.clone(),
                    history: Vec::new(),
                };

                release.install(self.client.clone()).await?;
                self.put_release_state(release.info.name.as_str(), &state)
                    .await?;
            }

            Some(mut state) => {
                let old_release = Release {
                    info: release.info.clone(),
                    objects: state.current.clone(),
                };

                release.upgrade(&old_release, self.client.clone()).await?;

                state.history.insert(0, state.current);
                state.current = release.objects.clone();

                self.put_release_state(release.info.name.as_str(), &state)
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
}
