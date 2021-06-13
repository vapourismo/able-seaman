use crate::k8s::apply_dynamic;
use crate::k8s::create_dynamic;
use crate::k8s::delete_dynamic;
use crate::release::DynamicError;
use crate::release::DynamicObject;
use kube::Client;

pub trait Rollbackable {
    fn to_rollback(&self) -> (Action, &DynamicObject);
}

#[derive(Debug)]
pub enum Action {
    Create,
    Apply,
    Delete,
}

#[derive(Debug)]
pub struct Error {
    error: DynamicError,
    action: Action,
    object: DynamicObject,
}

#[derive(Debug)]
pub struct Plan<'a> {
    creations: Vec<&'a DynamicObject>,
    upgrades: Vec<&'a DynamicObject>,
    deletions: Vec<&'a DynamicObject>,
}

impl<'a> Plan<'a> {
    pub fn new() -> Self {
        Plan {
            creations: Vec::new(),
            upgrades: Vec::new(),
            deletions: Vec::new(),
        }
    }

    pub async fn execute(&self, mut client: Client) -> Result<Client, Error> {
        let with_error = |action: Action, object: &DynamicObject| {
            let object = object.clone();
            move |error| Error {
                error,
                action,
                object,
            }
        };

        for creation in &self.creations {
            client = create_dynamic(client, creation)
                .await
                .map_err(with_error(Action::Create, creation))?
                .client;
        }

        for upgrade in &self.upgrades {
            client = apply_dynamic(client, upgrade)
                .await
                .map_err(with_error(Action::Apply, upgrade))?
                .client;
        }

        for deletion in &self.deletions {
            client = delete_dynamic(client, deletion)
                .await
                .map_err(with_error(Action::Delete, deletion))?;
        }

        Ok(client)
    }

    pub fn register<T: Rollbackable>(&mut self, action: &'a T) {
        match action.to_rollback() {
            (Action::Create, object) => {
                self.creations.push(object);
            }

            (Action::Apply, object) => {
                self.upgrades.push(object);
            }

            (Action::Delete, object) => {
                self.deletions.push(object);
            }
        }
    }
}
