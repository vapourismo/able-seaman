use crate::k8s::transaction;
use crate::objects::Object;
use std::error;
use std::fmt;

pub trait Rollbackable {
    fn to_rollback(&self) -> (transaction::Action, &Object);
}

#[derive(Debug)]
pub struct Error {
    pub error: transaction::Error,
    pub action: transaction::Action,
    pub object: Object,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            formatter,
            "Error during rollback while trying to {}: {}",
            self.action, self.error
        )
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug)]
pub struct Plan<'a> {
    creations: Vec<&'a Object>,
    upgrades: Vec<&'a Object>,
    deletions: Vec<&'a Object>,
}

impl<'a> Plan<'a> {
    pub fn new() -> Self {
        Plan {
            creations: Vec::new(),
            upgrades: Vec::new(),
            deletions: Vec::new(),
        }
    }

    pub async fn execute(&self, mut client: kube::Client) -> Result<kube::Client, Error> {
        let with_error = |action: transaction::Action, object: &Object| {
            let object = object.clone();
            move |error| Error {
                error,
                action,
                object,
            }
        };

        for creation in &self.creations {
            client = transaction::create_object(client, creation)
                .await
                .map_err(with_error(transaction::Action::Create, creation))?
                .client;
        }

        for upgrade in &self.upgrades {
            client = transaction::apply_object(client, upgrade)
                .await
                .map_err(with_error(transaction::Action::Apply, upgrade))?
                .client;
        }

        for deletion in &self.deletions {
            client = transaction::delete_object(client, deletion)
                .await
                .map_err(with_error(transaction::Action::Delete, deletion))?;
        }

        Ok(client)
    }

    pub fn register<T: Rollbackable>(&mut self, action: &'a T) {
        match action.to_rollback() {
            (transaction::Action::Create, object) => {
                self.creations.push(object);
            }

            (transaction::Action::Apply, object) => {
                self.upgrades.push(object);
            }

            (transaction::Action::Delete, object) => {
                self.deletions.push(object);
            }
        }
    }
}
