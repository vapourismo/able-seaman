pub mod manager;
pub mod plan;
pub mod rollback;

use crate::k8s::lock::Lock;
use crate::k8s::transaction;
use crate::release::plan::ReleasePlan;
use crate::utils::fs;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::core::DynamicObject;
use serde::{Deserialize, Serialize};
use std::collections::hash_map;
use std::collections::BTreeMap;
use std::fs::File;
use std::hash::Hash;
use std::hash::Hasher;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub enum Error {
    RollbackError {
        error: rollback::Error,
        cause: transaction::Error,
    },

    ReleaseError {
        error: transaction::Error,
    },
}

#[derive(Debug)]
pub enum BuildError {
    DuplicateObject(String),
    ObjectWithoutName(Box<DynamicObject>),
    IOError(io::Error),
    YAMLError(serde_yaml::Error),
}

impl From<serde_yaml::Error> for BuildError {
    fn from(error: serde_yaml::Error) -> BuildError {
        BuildError::YAMLError(error)
    }
}

impl From<io::Error> for BuildError {
    fn from(error: io::Error) -> BuildError {
        BuildError::IOError(error)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    name: String,
    pub(crate) objects: Objects,
}

pub type Objects = BTreeMap<String, DynamicObject>;

impl Release {
    pub fn new(name: String) -> Self {
        Release {
            name,
            objects: BTreeMap::new(),
        }
    }

    #[allow(clippy::needless_lifetimes)]
    pub async fn lock<'a>(
        &self,
        api: &'a kube::Api<ConfigMap>,
    ) -> Result<Lock<'a, ConfigMap>, kube::Error> {
        Lock::new(api, format!("{}-lock", self.name)).await
    }

    pub fn add_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), BuildError>
    where
        SomeRead: io::Read,
    {
        for document in serde_yaml::Deserializer::from_reader(input) {
            let object = DynamicObject::deserialize(document)?;

            if let Some(name) = object.metadata.name.clone() {
                if let Some(_old_object) = self.objects.insert(name.clone(), object) {
                    return Err(BuildError::DuplicateObject(name));
                }
            } else {
                return Err(BuildError::ObjectWithoutName(Box::new(object)));
            }
        }

        Ok(())
    }

    pub fn add_objects_from_path(&mut self, input: &Path) -> Result<(), BuildError> {
        for file in fs::list_files(input)? {
            let file = File::open(file.as_path())?;
            self.add_objects(file)?;
        }

        Ok(())
    }

    pub async fn upgrade(
        &self,
        old: &Self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&self.objects, &old.objects);
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub async fn install(
        &self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&self.objects, &BTreeMap::new());
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub async fn uninstall(
        &self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&BTreeMap::new(), &self.objects);
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub fn hash_value(&self) -> u64 {
        let mut hasher = hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

impl Hash for Release {
    fn hash<SomeHasher>(&self, hasher: &mut SomeHasher)
    where
        SomeHasher: std::hash::Hasher,
    {
        self.name.hash(hasher);

        for (name, object) in &self.objects {
            name.hash(hasher);

            match serde_json::to_string(object) {
                Ok(json) => json.hash(hasher),
                Err(_error) => {}
            }
        }
    }
}
