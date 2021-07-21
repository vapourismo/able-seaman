use crate::identifier::Identifier;
use crate::k8s::api_resource::ToApiResource;
use crate::k8s::api_resource::TryToApiResource;
use crate::utils::fs::list_files;
use kube::core::ApiResource;
use kube::core::DynamicObject;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::borrow::Cow;
use std::collections::hash_map;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::io;
use std::path::Path;

/// Clone of ApiResource that supports Serialize and Deserialize
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(remote = "ApiResource")]
struct SerDeApiResource {
    group: String,
    version: String,
    api_version: String,
    kind: String,
    plural: String,
}

/// A deployable object
#[derive(Clone, Debug)]
pub struct Object {
    pub api_resource: ApiResource,
    pub dyn_object: DynamicObject,
}

impl Object {
    /// Try to convert a DynamicObject into an Object.
    pub fn try_from_dynamic_object(dyn_object: DynamicObject) -> Option<Self> {
        dyn_object.try_to_api_resource().map(|api_resource| Object {
            api_resource,
            dyn_object,
        })
    }

    /// Get the name of the underlying Object.
    pub fn name(&self) -> Option<&String> {
        self.dyn_object.metadata.name.as_ref()
    }
}

impl ToApiResource for Object {
    fn to_api_resource(&self) -> ApiResource {
        self.api_resource.clone()
    }
}

impl TryFrom<DynamicObject> for Object {
    type Error = String;

    fn try_from(dyn_object: DynamicObject) -> Result<Self, Self::Error> {
        Object::try_from_dynamic_object(dyn_object)
            .ok_or_else(|| "Unable to extract ApiResource from DynamicObject".to_string())
    }
}

impl kube::Resource for Object {
    type DynamicType = <DynamicObject as kube::Resource>::DynamicType;

    fn kind(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::kind(dt)
    }

    fn group(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::group(dt)
    }

    fn version(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::version(dt)
    }

    fn plural(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::plural(dt)
    }

    fn meta(&self) -> &kube::core::ObjectMeta {
        self.dyn_object.meta()
    }

    fn meta_mut(&mut self) -> &mut kube::core::ObjectMeta {
        self.dyn_object.meta_mut()
    }
}

impl Serialize for Object {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.dyn_object.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        DynamicObject::deserialize(deserializer)
            .and_then(|dyn_object| Object::try_from(dyn_object).map_err(serde::de::Error::custom))
    }
}

/// Deployable collection of objects
#[derive(Debug, Clone)]
pub struct Objects {
    inner: HashMap<Identifier, Object>,
}

impl Objects {
    /// Construct an empty collection of objects.
    pub fn empty() -> Self {
        Objects {
            inner: HashMap::new(),
        }
    }

    /// Is there an object associated with the given identifier?
    pub fn contains(&self, ident: &Identifier) -> bool {
        self.inner.contains_key(ident)
    }

    /// Provide a borrowing iterator.
    pub fn iter(&self) -> hash_map::Iter<'_, Identifier, Object> {
        self.inner.iter()
    }

    /// Find an object associated with the given identifier.
    pub fn get(&self, key: &Identifier) -> Option<&Object> {
        self.inner.get(key)
    }
}

impl Default for Objects {
    fn default() -> Self {
        Self::empty()
    }
}

impl IntoIterator for Objects {
    type Item = <HashMap<Identifier, Object> as IntoIterator>::Item;

    type IntoIter = <HashMap<Identifier, Object> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Objects {
    type Item = <&'a HashMap<Identifier, Object> as IntoIterator>::Item;

    type IntoIter = <&'a HashMap<Identifier, Object> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        <&'a HashMap<Identifier, Object> as IntoIterator>::into_iter(&self.inner)
    }
}

impl From<HashMap<Identifier, Object>> for Objects {
    fn from(inner: HashMap<Identifier, Object>) -> Self {
        Objects { inner }
    }
}

impl From<Objects> for HashMap<Identifier, Object> {
    fn from(other: Objects) -> Self {
        other.inner
    }
}

/// Helper type for Serialize and Deserialize for Objects
#[derive(Serialize, Deserialize)]
struct SerDeObjectsEntry {
    identifier: Identifier,
    object: Object,
}

impl Serialize for Objects {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner
            .iter()
            .map(|(identifier, object)| SerDeObjectsEntry {
                identifier: identifier.clone(),
                object: object.clone(),
            })
            .collect::<Vec<SerDeObjectsEntry>>()
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Objects {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner = Vec::deserialize(deserializer)?
            .into_iter()
            .map(|entry: SerDeObjectsEntry| (entry.identifier, entry.object))
            .collect();

        Ok(Objects { inner })
    }
}

/// An error that may occur while building a collection of deployable objects
#[derive(Debug)]
pub enum BuilderError {
    /// Encountered at least 2 objects which shared the same identifier (e.g. name and kind)
    DuplicateObject { identifier: Identifier },

    /// One object did not have a name in the metadata section
    ObjectWithoutName { object: Box<Object> },

    /// Encountered a bad object
    BadDynamicObject { error: String },

    /// Failed to list the contents of a directory
    ListFilesError { path: Box<Path>, error: io::Error },

    /// File could not be opened for reading
    OpenFileError { path: Box<Path>, error: io::Error },

    /// Object made of faulty YAML
    DeserializeError { error: serde_yaml::Error },
}

impl From<serde_yaml::Error> for BuilderError {
    fn from(error: serde_yaml::Error) -> BuilderError {
        BuilderError::DeserializeError { error }
    }
}

/// Builder for Objects
#[derive(Debug)]
pub struct Builder {
    objects: HashMap<Identifier, Object>,
}

impl Builder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Builder {
            objects: HashMap::new(),
        }
    }

    /// Add a DynamicObject.
    pub fn add_dynamic_object(&mut self, dyn_object: DynamicObject) -> Result<(), BuilderError> {
        let object = Object::try_from(dyn_object)
            .map_err(|error| BuilderError::BadDynamicObject { error })?;

        let name = object
            .name()
            .ok_or_else(|| BuilderError::ObjectWithoutName {
                object: Box::new(object.clone()),
            })?
            .clone();

        let identifier = Identifier::from_api_resource(name, &object.api_resource);

        if self.objects.insert(identifier.clone(), object).is_some() {
            return Err(BuilderError::DuplicateObject { identifier });
        }

        Ok(())
    }

    /// Read objects from a YAML document.
    pub fn read_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), BuilderError>
    where
        SomeRead: io::Read,
    {
        for document in serde_yaml::Deserializer::from_reader(input) {
            let object = DynamicObject::deserialize(document)?;
            self.add_dynamic_object(object)?;
        }

        Ok(())
    }

    /// Read objects from a file or files. If the given path is a directory, it will be traversed
    /// and all files, including in any subdirectories will be read.
    pub fn read_objects_from_path(&mut self, input: &Path) -> Result<(), BuilderError> {
        let files = list_files(input).map_err(|error| BuilderError::ListFilesError {
            path: input.to_owned().into_boxed_path(),
            error,
        })?;

        for file in files {
            let file = File::open(file.as_path()).map_err(|error| BuilderError::OpenFileError {
                path: file.into_boxed_path(),
                error,
            })?;

            self.read_objects(file)?;
        }

        Ok(())
    }

    /// Finalize the building process.
    pub fn finish(self) -> Objects {
        Objects::from(self.objects)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
