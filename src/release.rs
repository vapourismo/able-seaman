use crate::errors::GeneralError;
use crate::k8s::apply_dynamic;
use crate::k8s::create_dynamic;
use crate::k8s::delete_dynamic;
use crate::k8s::DynamicError;
use crate::k8s::Lock;
use crate::k8s::ObjectType;
use crate::k8s::TaggableObject;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::core::DynamicObject;
use kube::Api;
use kube::Client;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::hash::Hash;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

fn list_contents_vec(paths: &mut Vec<PathBuf>, path: &Path) -> Result<(), GeneralError> {
    if path.is_dir() {
        for entry in path.read_dir()? {
            let dir = entry?.path();
            list_contents_vec(paths, dir.as_path())?;
        }
    } else if path.exists() {
        paths.push(path.to_path_buf());
    } else {
        return Err(GeneralError::FileNotFound(
            path.to_owned().into_boxed_path(),
        ));
    }

    Ok(())
}

fn list_contents(path: &Path) -> Result<Vec<PathBuf>, GeneralError> {
    let mut paths = Vec::new();
    list_contents_vec(&mut paths, path)?;
    Ok(paths)
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub name: String,
}

pub type Objects = BTreeMap<String, DynamicObject>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub info: ReleaseInfo,
    pub objects: Objects,
}

impl Release {
    pub fn new(info: ReleaseInfo) -> Self {
        Release {
            info,
            objects: BTreeMap::new(),
        }
    }

    pub async fn lock<'a>(
        &self,
        api: &'a Api<ConfigMap>,
    ) -> Result<Lock<'a, ConfigMap>, kube::Error> {
        Lock::new(api, format!("{}-lock", self.info.name)).await
    }

    pub fn to_config_map(&self) -> Result<ConfigMap, serde_json::Error> {
        let mut data = BTreeMap::new();
        data.insert("release".to_string(), serde_json::to_string(self)?);

        let mut config_map = ConfigMap::default();
        config_map.data = Some(data);
        config_map.metadata.name = Some(self.info.name.clone());
        config_map.tag(ObjectType::Release);

        Ok(config_map)
    }

    pub fn ingest_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), GeneralError>
    where
        SomeRead: Read,
    {
        for document in serde_yaml::Deserializer::from_reader(input) {
            let object = DynamicObject::deserialize(document)?;

            if let Some(name) = object.metadata.name.clone() {
                if let Some(_old_object) = self.objects.insert(name.clone(), object) {
                    return Err(GeneralError::DuplicateObject(name));
                }
            } else {
                return Err(GeneralError::ObjectWithoutName(object));
            }
        }

        Ok(())
    }

    pub fn ingest_objects_from_path(&mut self, input: &Path) -> Result<(), GeneralError> {
        for file in list_contents(input)? {
            let file = File::open(file.as_path())?;
            self.ingest_objects(file)?;
        }

        Ok(())
    }

    pub async fn upgrade(&self, old: &Self, client: Client) -> Result<Client, ReleaseError> {
        let plan = ReleasePlan::new(&self.objects, &old.objects);
        Ok(plan.execute(client).await?)
    }

    pub async fn install(&self, client: Client) -> Result<Client, ReleaseError> {
        let plan = ReleasePlan::new(&self.objects, &BTreeMap::new());
        Ok(plan.execute(client).await?)
    }
}

impl Hash for Release {
    fn hash<SomeHasher>(&self, hasher: &mut SomeHasher)
    where
        SomeHasher: std::hash::Hasher,
    {
        self.info.hash(hasher);

        for (name, object) in &self.objects {
            name.hash(hasher);

            match serde_json::to_string(object) {
                Ok(json) => json.hash(hasher),
                Err(_error) => {}
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Create {
    new: DynamicObject,
}

#[derive(Clone, Debug)]
struct Upgrade {
    new: DynamicObject,
    old: DynamicObject,
}

#[derive(Clone, Debug)]
struct Delete {
    old: DynamicObject,
}

#[derive(Debug)]
enum Action {
    Create(Create),
    Upgrade(Upgrade),
    Delete(Delete),
}

#[derive(Clone, Debug)]
pub struct ReleasePlan {
    creations: Vec<Create>,
    upgrades: Vec<Upgrade>,
    deletions: Vec<Delete>,
}

#[derive(Debug)]
pub enum ReleaseError {
    RollbackError {
        rollback_error: RollbackError,
        error: DynamicError,
        action: Action,
    },
    ActionError {
        error: DynamicError,
        action: Action,
    },
}

impl ReleasePlan {
    pub fn new(new_objects: &Objects, old_objects: &Objects) -> Self {
        // Find things to create.
        let creations = new_objects
            .iter()
            .filter(|(key, _)| !old_objects.contains_key(*key))
            .map(|(_, new)| Create { new: new.clone() })
            .collect();

        // Find things to upgrade.
        let upgrades = new_objects
            .iter()
            .filter_map(|(key, new)| {
                old_objects.get(key).map(|current| Upgrade {
                    new: new.clone(),
                    old: current.clone(),
                })
            })
            .collect();

        // Find things to delete.
        let deletions = old_objects
            .iter()
            .filter(|(key, _)| !new_objects.contains_key(*key))
            .map(|(_, value)| Delete { old: value.clone() })
            .collect();

        ReleasePlan {
            creations,
            upgrades,
            deletions,
        }
    }

    pub async fn execute(&self, mut client: Client) -> Result<Client, ReleaseError> {
        let rollback_client = client.clone();
        let mut rollback_creations = Vec::new();
        let mut rollback_upgrades = Vec::new();
        let mut rollback_deletions = Vec::new();

        for creation in &self.creations {
            let result = or_rollback(
                rollback_client.clone(),
                &rollback_creations,
                &rollback_upgrades,
                &rollback_deletions,
                Action::Create(creation.clone()),
                create_dynamic(client, &creation.new).await,
            )
            .await?;
            client = result.client;
            rollback_deletions.push(&creation.new);
        }

        for upgrade in &self.upgrades {
            let result = or_rollback(
                rollback_client.clone(),
                &rollback_creations,
                &rollback_upgrades,
                &rollback_deletions,
                Action::Upgrade(upgrade.clone()),
                apply_dynamic(client, &upgrade.new).await,
            )
            .await?;
            client = result.client;
            rollback_upgrades.push(&upgrade.old);
        }

        for deletion in &self.deletions {
            client = or_rollback(
                rollback_client.clone(),
                &rollback_creations,
                &rollback_upgrades,
                &rollback_deletions,
                Action::Delete(deletion.clone()),
                delete_dynamic(client, &deletion.old).await,
            )
            .await?;
            rollback_creations.push(&deletion.old);
        }

        Ok(client)
    }
}

#[derive(Debug)]
pub enum RollbackError {
    DynamicError(DynamicError),
}

impl From<DynamicError> for RollbackError {
    fn from(error: DynamicError) -> Self {
        RollbackError::DynamicError(error)
    }
}

async fn rollback(
    mut client: Client,
    creations: &Vec<&DynamicObject>,
    upgrades: &Vec<&DynamicObject>,
    deletions: &Vec<&DynamicObject>,
) -> Result<(), RollbackError> {
    for creation in creations {
        client = create_dynamic(client, creation).await?.client;
    }

    for upgrade in upgrades {
        client = apply_dynamic(client, upgrade).await?.client;
    }

    for deletion in deletions {
        client = delete_dynamic(client, deletion).await?;
    }

    Ok(())
}

async fn or_rollback<T>(
    client: Client,
    creations: &Vec<&DynamicObject>,
    upgrades: &Vec<&DynamicObject>,
    deletions: &Vec<&DynamicObject>,
    action: Action,
    result: Result<T, DynamicError>,
) -> Result<T, ReleaseError> {
    match result {
        Err(error) => {
            let rollback_result = rollback(client, &creations, &upgrades, &deletions).await;
            Err(match rollback_result {
                Ok(_) => ReleaseError::ActionError { error, action },
                Err(rollback_error) => ReleaseError::RollbackError {
                    rollback_error,
                    error,
                    action,
                },
            })
        }

        Ok(result) => Ok(result),
    }
}
