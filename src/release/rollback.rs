use crate::k8s::apply_dynamic;
use crate::k8s::create_dynamic;
use crate::k8s::delete_dynamic;
use crate::release::DynamicError;
use crate::release::DynamicObject;
use kube::Client;

pub trait Rollbackable {
    fn to_rollback(&self) -> (RollbackAction, &DynamicObject);
}

#[derive(Debug)]
pub enum RollbackAction {
    Create,
    Apply,
    Delete,
}

#[derive(Debug)]
pub struct RollbackError {
    error: DynamicError,
    action: RollbackAction,
    object: DynamicObject,
}

pub struct RollbackPlan<'a> {
    creations: Vec<&'a DynamicObject>,
    upgrades: Vec<&'a DynamicObject>,
    deletions: Vec<&'a DynamicObject>,
}

impl<'a> RollbackPlan<'a> {
    pub fn new() -> Self {
        RollbackPlan {
            creations: Vec::new(),
            upgrades: Vec::new(),
            deletions: Vec::new(),
        }
    }

    pub async fn execute(&self, mut client: Client) -> Result<Client, RollbackError> {
        let with_error = |action: RollbackAction, object: &DynamicObject| {
            let object = object.clone();
            move |error| RollbackError {
                error,
                action,
                object,
            }
        };

        for creation in &self.creations {
            client = create_dynamic(client, creation)
                .await
                .map_err(with_error(RollbackAction::Create, creation))?
                .client;
        }

        for upgrade in &self.upgrades {
            client = apply_dynamic(client, upgrade)
                .await
                .map_err(with_error(RollbackAction::Apply, upgrade))?
                .client;
        }

        for deletion in &self.deletions {
            client = delete_dynamic(client, deletion)
                .await
                .map_err(with_error(RollbackAction::Delete, deletion))?;
        }

        Ok(client)
    }

    pub fn register<T: Rollbackable>(&mut self, action: &'a T) {
        match action.to_rollback() {
            (RollbackAction::Create, object) => {
                self.creations.push(object);
            }

            (RollbackAction::Apply, object) => {
                self.upgrades.push(object);
            }

            (RollbackAction::Delete, object) => {
                self.deletions.push(object);
            }
        }
    }
}
