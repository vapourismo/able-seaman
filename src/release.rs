pub mod plan;
pub mod rollback;
pub mod verify;

use crate::identifier::Identifier;
use crate::k8s::lock::Lock;
use crate::k8s::transaction;
use crate::objects::Objects;
use crate::release::plan::ReleasePlan;
use k8s_openapi::api::core::v1::ConfigMap;
use std::collections::hash_map;
use std::hash::Hash;
use std::hash::Hasher;

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
        let plan = ReleasePlan::new(&self.name, &self.objects, &Objects::empty());
        client = plan.execute(client).await?;
        Ok((client, plan))
    }

    pub async fn uninstall(
        &self,
        mut client: kube::Client,
    ) -> Result<(kube::Client, ReleasePlan), Error> {
        let plan = ReleasePlan::new(&self.name, &Objects::empty(), &self.objects);
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
