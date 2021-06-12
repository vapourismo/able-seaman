use crate::errors::GeneralError;
use crate::k8s::tag_object;
use crate::k8s::Lock;
use crate::k8s::ObjectType;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::core::DynamicObject;
use kube::Api;
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
        tag_object(&mut config_map, ObjectType::Release);

        Ok(config_map)
    }

    pub fn ingest_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), GeneralError>
    where
        SomeRead: Read,
    {
        self.objects.append(&mut ingest_objects(input)?);

        Ok(())
    }

    pub fn ingest_objects_from_path(&mut self, input: &Path) -> Result<(), GeneralError> {
        for file in list_contents(input)? {
            let file = File::open(file.as_path())?;
            self.ingest_objects(file)?;
        }

        Ok(())
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

pub type Objects = BTreeMap<String, DynamicObject>;

pub fn ingest_objects<SomeRead>(input: SomeRead) -> Result<Objects, GeneralError>
where
    SomeRead: std::io::Read,
{
    let mut objects = BTreeMap::new();

    for document in serde_yaml::Deserializer::from_reader(input) {
        let object = DynamicObject::deserialize(document)?;

        if let Some(name) = object.metadata.name.clone() {
            if let Some(_old_object) = objects.insert(name.clone(), object) {
                return Err(GeneralError::DuplicateObject(name));
            }
        } else {
            return Err(GeneralError::ObjectWithoutName(object));
        }
    }

    Ok(objects)
}
