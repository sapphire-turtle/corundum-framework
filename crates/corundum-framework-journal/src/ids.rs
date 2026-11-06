//! The ids, the origin and the actor of a batch.

use std::fmt;

use grain_id::GrainId;
use serde::{Deserialize, Serialize};

macro_rules! id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub GrainId);

        impl $name {
            /// A new random id.
            pub fn random() -> Self {
                Self(GrainId::random())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

id! {
    /// The id of a batch of operations.
    BatchId
}

id! {
    /// The id of one recorded operation.
    OpId
}

/// What a batch came from, such as the name of the method or the app that made it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Origin(String);

impl Origin {
    /// An origin with this name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name of the origin.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Who made a batch.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Actor {
    /// The principal the batch was made for.
    pub principal: String,
    /// Whether an agent acted for the principal, rather than the person themselves.
    pub agent: bool,
}

impl Actor {
    /// A person acting for themselves.
    pub fn person(principal: impl Into<String>) -> Self {
        Self {
            principal: principal.into(),
            agent: false,
        }
    }

    /// An agent acting for a principal.
    pub fn agent(principal: impl Into<String>) -> Self {
        Self {
            principal: principal.into(),
            agent: true,
        }
    }
}
