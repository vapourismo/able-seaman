use crate::identifier::Identifier;
use crate::k8s;
use crate::k8s::annotations::WithAnnotations;
use crate::k8s::labels::WithLabels;
use crate::k8s::transaction;
use crate::objects;
use crate::release;
use crate::release::plan;
use crate::release::verify;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::Resource;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::str;

#[derive(Debug)]
pub enum Error {
    Kube(kube::Error),

    ReleaseState(Box<ReleaseStateError>),

    Release {
        state: ReleaseState,
        error: Box<release::Error>,
    },
}

impl From<kube::Error> for Error {
    fn from(error: kube::Error) -> Self {
        Error::Kube(error)
    }
}

impl From<ReleaseStateError> for Error {
    fn from(error: ReleaseStateError) -> Self {
        Error::ReleaseState(Box::new(error))
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
                        .map_err(|error| Error::Release {
                            error: Box::new(error),
                            state: state.clone(),
                        })?;

                if let Err(err_cause) = state.apply(&self.config_maps, name.as_str()).await {
                    plan.undo()
                        .execute(self.client.clone())
                        .await
                        .map_err(|error| Error::Release {
                            error: Box::new(error),
                            state: state.clone(),
                        })?;
                    return Err(err_cause.into());
                }

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
                    .map_err(|error| Error::Release {
                        error: Box::new(error),
                        state: state.clone(),
                    })?;

                state.history.insert(0, state.current);
                state.current = release.objects().clone();

                if let Err(err_cause) = state.apply(&self.config_maps, name.as_str()).await {
                    plan.undo()
                        .execute(self.client.clone())
                        .await
                        .map_err(|error| Error::Release {
                            error: Box::new(error),
                            state: state.clone(),
                        })?;
                    return Err(err_cause.into());
                }

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
                .map_err(|error| Error::Release {
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

    pub async fn verify(&self, release_name: String) -> Result<(), VerificationError> {
        let state = ReleaseState::get(&self.config_maps, release_name.as_str())
            .await?
            .ok_or(VerificationError::NoDeployedRelease)?;

        let real_objects =
            verify::find_release_objects(self.client.clone(), release_name.clone()).await?;

        for (identifier, desired) in state.current {
            let desired = plan::ReleasePlan::tag_object(release_name.clone(), desired);

            let reality = real_objects
                .get(&identifier)
                .ok_or_else(|| VerificationError::MissingObject(identifier.clone()))?;

            let desired_meta = desired.meta();
            let reality_meta = reality.meta();

            if !verify::check_mapping(&desired_meta.annotations, &reality_meta.annotations) {
                return Err(VerificationError::MismatchingAnnotations {
                    identifier: identifier.clone(),
                    desired: desired_meta.annotations.clone(),
                    reality: reality_meta.annotations.clone(),
                });
            }

            if !verify::check_mapping(&desired_meta.labels, &reality_meta.labels) {
                return Err(VerificationError::MismatchingLabels {
                    identifier: identifier.clone(),
                    desired: desired_meta.labels.clone(),
                    reality: reality_meta.labels.clone(),
                });
            }

            verify::check_value(
                &desired.dyn_object.data,
                &reality.dyn_object.data,
                VecDeque::new(),
            )
            .map_err(|path| VerificationError::MismatchingData { path })?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum VerificationError {
    ReleaseStateError(ReleaseStateError),
    KubeError(kube::Error),
    NoDeployedRelease,
    MissingObject(Identifier),
    MismatchingLabels {
        identifier: Identifier,
        desired: BTreeMap<String, String>,
        reality: BTreeMap<String, String>,
    },
    MismatchingAnnotations {
        identifier: Identifier,
        desired: BTreeMap<String, String>,
        reality: BTreeMap<String, String>,
    },
    MismatchingData {
        path: VecDeque<String>,
    },
}

impl From<kube::Error> for VerificationError {
    fn from(error: kube::Error) -> Self {
        VerificationError::KubeError(error)
    }
}

impl From<ReleaseStateError> for VerificationError {
    fn from(error: ReleaseStateError) -> Self {
        VerificationError::ReleaseStateError(error)
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ReleaseState {
    current: objects::Objects,
    history: Vec<objects::Objects>,
}

impl ReleaseState {
    fn from_config_map(config_map: &ConfigMap) -> Result<Self, ReleaseStateError> {
        let data = config_map
            .data
            .get("release_state")
            .ok_or_else(|| ReleaseStateError::CorruptReleaseState(config_map.clone()))?;

        Ok(serde_json::from_str(data.as_str())?)
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

        transaction::apply(api, &config_map)
            .await
            .map_err(ReleaseStateError::UpdateError)?;

        Ok(())
    }
}
