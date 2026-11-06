//! Operations and a context for the journal's tests.

// Each test file uses a part of these.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};

use corundum_framework_journal::{
    Actor, Applied, Journal, JournalConfig, MemoryStore, OpKind, Operation, Origin, Planned,
    Registry, Skip,
};
use serde::{Deserialize, Deserializer, Serialize};

/// The state the test operations change.
#[derive(Default)]
pub struct Ctx {
    /// The values, by key.
    pub values: Mutex<BTreeMap<String, i64>>,
    /// What a `Wait` blocks on.
    pub gate: Mutex<Option<mpsc::Receiver<()>>>,
}

impl Ctx {
    /// The value under `key`.
    pub fn value(&self, key: &str) -> Option<i64> {
        self.values.lock().unwrap().get(key).copied()
    }

    /// Sets up the gate, and gives its sender.
    ///
    /// The next `Wait` takes each message at once and runs until the sender is dropped. The
    /// gate is a rendezvous, so a `send` returns only once the `Wait` is running.
    pub fn gate(&self) -> mpsc::SyncSender<()> {
        let (tx, rx) = mpsc::sync_channel(0);
        *self.gate.lock().unwrap() = Some(rx);
        tx
    }
}

/// The journal's context in the tests.
pub type Shared = Arc<Ctx>;

/// The plan of a `Set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetPlan {
    /// The key.
    pub key: String,
    /// The new value; `None` removes the key.
    pub to: Option<i64>,
    /// What the value must be now (`Some(None)`: absent); `None` checks nothing.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "some"
    )]
    pub expect: Option<Option<i64>>,
}

fn some<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<i64>>, D::Error> {
    Option::<i64>::deserialize(d).map(Some)
}

/// What a `Set` did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetDone {
    /// The key.
    pub key: String,
    /// The value before.
    pub from: Option<i64>,
    /// The value after.
    pub to: Option<i64>,
}

/// Sets or removes a value.
pub struct Set;

impl OpKind for Set {
    const KIND: &'static str = "test.set";
    type Plan = SetPlan;
    type Done = SetDone;
}

fn precondition(values: &BTreeMap<String, i64>, plan: &SetPlan) -> Result<Option<i64>, Skip> {
    let current = values.get(&plan.key).copied();
    match plan.expect {
        Some(e) if e != current => Err(Skip::PreconditionFailed),
        _ => Ok(current),
    }
}

impl Operation<Shared> for Set {
    fn check(&self, ctx: &Shared, plan: &SetPlan) -> Result<(), Skip> {
        precondition(&ctx.values.lock().unwrap(), plan).map(|_| ())
    }

    fn apply(&self, ctx: &Shared, plan: &SetPlan) -> Result<Applied<SetDone>, Skip> {
        let mut values = ctx.values.lock().unwrap();
        let from = precondition(&values, plan)?;
        match plan.to {
            Some(v) => values.insert(plan.key.clone(), v),
            None => values.remove(&plan.key),
        };
        Ok(Applied::new(SetDone {
            key: plan.key.clone(),
            from,
            to: plan.to,
        }))
    }

    fn inverse(&self, done: &SetDone) -> Option<SetPlan> {
        Some(SetPlan {
            key: done.key.clone(),
            to: done.from,
            expect: Some(done.to),
        })
    }
}

/// A `Set` of `key` to `to`, checking nothing.
pub fn set(key: &str, to: i64) -> Planned {
    Planned::new::<Set>(&SetPlan {
        key: key.into(),
        to: Some(to),
        expect: None,
    })
}

/// A `Set` of `key` to `to` that expects the value `expect`.
pub fn set_expect(key: &str, to: i64, expect: Option<i64>) -> Planned {
    Planned::new::<Set>(&SetPlan {
        key: key.into(),
        to: Some(to),
        expect: Some(expect),
    })
}

/// The plan of a `WithChild`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WithChildPlan {
    /// The key.
    pub key: String,
}

/// Sets `key` to 1, with the child `Set` of `{key}.side` to 1.
pub struct WithChild;

impl OpKind for WithChild {
    const KIND: &'static str = "test.with_child";
    type Plan = WithChildPlan;
    type Done = SetDone;
}

impl Operation<Shared> for WithChild {
    fn apply(&self, ctx: &Shared, plan: &WithChildPlan) -> Result<Applied<SetDone>, Skip> {
        let from = ctx.values.lock().unwrap().insert(plan.key.clone(), 1);
        Ok(Applied {
            done: SetDone {
                key: plan.key.clone(),
                from,
                to: Some(1),
            },
            children: vec![set(&format!("{}.side", plan.key), 1)],
        })
    }

    fn inverse(&self, _done: &SetDone) -> Option<WithChildPlan> {
        None
    }
}

/// A `WithChild` on `key`.
pub fn with_child(key: &str) -> Planned {
    Planned::new::<WithChild>(&WithChildPlan { key: key.into() })
}

/// Panics.
pub struct Boom;

impl OpKind for Boom {
    const KIND: &'static str = "test.boom";
    type Plan = ();
    type Done = ();
}

impl Operation<Shared> for Boom {
    fn apply(&self, _ctx: &Shared, _plan: &()) -> Result<Applied<()>, Skip> {
        panic!("boom")
    }

    fn inverse(&self, _done: &()) -> Option<()> {
        None
    }
}

/// Blocks on the gate, if one is set up, until its sender is dropped.
pub struct Wait;

impl OpKind for Wait {
    const KIND: &'static str = "test.wait";
    type Plan = ();
    type Done = ();
}

impl Operation<Shared> for Wait {
    fn apply(&self, ctx: &Shared, _plan: &()) -> Result<Applied<()>, Skip> {
        let gate = ctx.gate.lock().unwrap().take();
        if let Some(rx) = gate {
            while rx.recv().is_ok() {}
        }
        Ok(Applied::new(()))
    }

    fn inverse(&self, _done: &()) -> Option<()> {
        None
    }
}

/// The GUI's origin.
pub fn gui() -> Origin {
    Origin::new("gui")
}

/// The person `u1`.
pub fn u1() -> Actor {
    Actor::person("u1")
}

/// A registry with all the test kinds.
pub fn registry() -> Registry<Shared> {
    let mut r = Registry::new();
    r.register(Set)
        .register(WithChild)
        .register(Boom)
        .register(Wait);
    r
}

/// A journal over a `MemoryStore`, with this config, and its context.
pub fn journal_with(config: JournalConfig) -> (Journal<Shared>, Shared) {
    let ctx = Shared::default();
    let j = Journal::new(registry(), ctx.clone(), MemoryStore::default(), config);
    (j, ctx)
}

/// A journal over a `MemoryStore`, with the default config, and its context.
pub fn journal() -> (Journal<Shared>, Shared) {
    journal_with(JournalConfig::default())
}
