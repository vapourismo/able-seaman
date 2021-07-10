pub mod plan;
pub mod rollback;
pub mod verify;

use crate::identifier::Identifier;
use crate::k8s::lock::Lock;
use crate::k8s::transaction;
use crate::release::plan::ReleasePlan;
use crate::utils::fs;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::core::DynamicObject;
use serde::Deserialize;
use std::collections::hash_map;
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hash;
use std::hash::Hasher;
use std::io;
use std::path::Path;

#[derive(Debug)]
pub enum BuildError {
    DuplicateObject(Identifier),
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

#[derive(Debug)]
pub struct Builder {
    objects: Objects,
}

impl Builder {
    pub fn new() -> Self {
        Builder {
            objects: HashMap::new(),
        }
    }

    pub fn add_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), BuildError>
    where
        SomeRead: io::Read,
    {
        for document in serde_yaml::Deserializer::from_reader(input) {
            let object = DynamicObject::deserialize(document)?;

            if let Some(identifier) = Identifier::from_resource(&object) {
                if self.objects.insert(identifier.clone(), object).is_some() {
                    return Err(BuildError::DuplicateObject(identifier));
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

    pub fn finish(self, name: String) -> Release {
        Release::from_objects(name, self.objects)
    }
}

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

#[derive(Clone, Debug)]
pub struct Release {
    name: String,
    objects: Objects,
}

pub type Objects = HashMap<Identifier, DynamicObject>;

impl Release {
    pub fn from_objects(name: String, objects: Objects) -> Self {
        Release { name, objects }
    }

    #[allow(clippy::needless_lifetimes)]
    pub async fn lock<'a>(
        &self,
        api: &'a kube::Api<ConfigMap>,
    ) -> Result<Lock<'a, ConfigMap>, kube::Error> {
        Lock::new(api, format!("{}-lock", self.name)).await
    }

    pub async fn upgrade(
        &self,
        old: &Self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&self.name, &self.objects, &old.objects);
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub async fn install(
        &self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&self.name, &self.objects, &HashMap::new());
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub async fn uninstall(
        &self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&self.name, &HashMap::new(), &self.objects);
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub fn hash_value(&self) -> u64 {
        let mut hasher = hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn objects(&self) -> &Objects {
        &self.objects
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
