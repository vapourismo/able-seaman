use crate::k8s::tag_object;
use crate::k8s::wait_for_deletion;
use crate::k8s::ObjectType;
use kube::api::DeleteParams;
use kube::api::PostParams;
use kube::Api;
use kube::Resource;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

pub struct Lock<'a, T>
where
    T: Clone + DeserializeOwned + Debug,
{
    api: &'a Api<T>,
    name: String,
}

impl<'a, T> Lock<'a, T>
where
    T: Resource + Default + Clone + Debug + DeserializeOwned + Serialize,
{
    pub async fn new(api: &'a Api<T>, name: String) -> Result<Lock<'a, T>, kube::Error> {
        Lock::new_with(api, name, <T as Default>::default()).await
    }

    pub async fn new_with(
        api: &'a Api<T>,
        name: String,
        mut lock_value: T,
    ) -> Result<Lock<'a, T>, kube::Error> {
        lock_value.meta_mut().name = Some(name.clone());
        tag_object(&mut lock_value, ObjectType::Lock);

        let _locked_value = loop {
            match api.create(&PostParams::default(), &lock_value).await {
                Err(kube::Error::Api(kube::error::ErrorResponse {
                    reason, code: 409, ..
                })) if reason == "AlreadyExists" => {
                    wait_for_deletion(&api, &name).await?;
                }

                result => {
                    break result?;
                }
            }
        };

        Ok(Lock { api, name })
    }
}

impl<'a, T> Drop for Lock<'a, T>
where
    T: Clone + DeserializeOwned + Debug,
{
    fn drop(&mut self) {
        let deletion = futures::executor::block_on(
            self.api
                .delete(self.name.as_str(), &DeleteParams::default()),
        );

        match deletion {
            Err(err) => {
                eprintln!("Failed to delete locking ConfigMap {}: {}", self.name, err);
            }

            _ => {}
        }
    }
}
