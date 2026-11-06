//! Operations, and the registry that runs them by kind.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Skip;

/// The kind of an operation, with the types of its plan and its result.
pub trait OpKind {
    /// The name the journal records the operation under.
    const KIND: &'static str;
    /// What the operation is to do.
    type Plan: Serialize + DeserializeOwned + Send;
    /// What the operation leaves behind for its undo.
    type Done: Serialize + DeserializeOwned + Send;
}

/// An operation that runs against a context `C`.
pub trait Operation<C>: OpKind + Send + Sync + 'static {
    /// Whether the plan could be applied now. The default accepts every plan.
    fn check(&self, _ctx: &C, _plan: &Self::Plan) -> Result<(), Skip> {
        Ok(())
    }

    /// Does what the plan says.
    fn apply(&self, ctx: &C, plan: &Self::Plan) -> Result<Applied<Self::Done>, Skip>;

    /// The plan that reverses what was done, or `None` if it cannot be reversed.
    fn inverse(&self, done: &Self::Done) -> Option<Self::Plan>;
}

/// What an applied operation leaves behind.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied<D> {
    /// The value the operation left for its undo.
    pub done: D,
    /// The operations it led to, to run after it.
    pub children: Vec<Planned>,
}

impl<D> Applied<D> {
    /// A result with no children.
    pub fn new(done: D) -> Self {
        Self {
            done,
            children: Vec::new(),
        }
    }
}

/// An operation to run, by kind, with its plan as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Planned {
    /// The kind of the operation.
    pub kind: String,
    /// Its plan.
    pub plan: Value,
}

impl Planned {
    /// A planned operation of kind `O`.
    ///
    /// # Panics
    ///
    /// Panics if serde cannot serialize the plan.
    pub fn new<O: OpKind>(plan: &O::Plan) -> Planned {
        Planned {
            kind: O::KIND.to_owned(),
            plan: serde_json::to_value(plan).expect("the plan serializes"),
        }
    }
}

/// Why a planned operation could not even be tried.
#[derive(Debug)]
pub(crate) enum Unparsed {
    /// No operation is registered under the kind.
    UnknownKind,
    /// The plan did not parse, with the reason.
    InvalidPlan(String),
}

type Inverting = Result<Result<Option<Value>, Skip>, Unparsed>;

type Applying = Result<Result<(Value, Vec<Planned>), Skip>, Unparsed>;

trait Erased<C>: Send + Sync {
    fn parse(&self, plan: &Value) -> Result<(), Unparsed>;
    fn check(&self, ctx: &C, plan: &Value) -> Result<Result<(), Skip>, Unparsed>;
    fn apply(&self, ctx: &C, plan: &Value) -> Applying;
    fn inverse(&self, done: &Value) -> Inverting;
}

fn panicked() -> Skip {
    Skip::Io("the operation panicked".into())
}

fn parse<T: DeserializeOwned>(v: &Value) -> Result<T, Unparsed> {
    serde_json::from_value(v.clone()).map_err(|e| Unparsed::InvalidPlan(e.to_string()))
}

impl<C, O: Operation<C>> Erased<C> for O {
    fn parse(&self, plan: &Value) -> Result<(), Unparsed> {
        parse::<O::Plan>(plan).map(|_| ())
    }

    fn check(&self, ctx: &C, plan: &Value) -> Result<Result<(), Skip>, Unparsed> {
        let plan: O::Plan = parse(plan)?;
        Ok(
            catch_unwind(AssertUnwindSafe(|| Operation::check(self, ctx, &plan)))
                .unwrap_or_else(|_| Err(panicked())),
        )
    }

    fn apply(&self, ctx: &C, plan: &Value) -> Applying {
        let plan: O::Plan = parse(plan)?;
        let result = catch_unwind(AssertUnwindSafe(|| Operation::apply(self, ctx, &plan)))
            .unwrap_or_else(|_| Err(panicked()));
        Ok(result.and_then(|a| {
            serde_json::to_value(&a.done)
                .map(|done| (done, a.children))
                .map_err(|e| Skip::Io(format!("the result did not serialize: {e}")))
        }))
    }

    fn inverse(&self, done: &Value) -> Inverting {
        let done: O::Done = parse(done)?;
        let plan = match catch_unwind(AssertUnwindSafe(|| Operation::inverse(self, &done))) {
            Ok(plan) => plan,
            Err(_) => return Ok(Err(panicked())),
        };
        Ok(match plan {
            None => Ok(None),
            Some(p) => serde_json::to_value(p)
                .map(Some)
                .map_err(|e| Skip::Io(e.to_string())),
        })
    }
}

/// The operations the journal can run, by kind.
pub struct Registry<C> {
    ops: HashMap<&'static str, Box<dyn Erased<C>>>,
}

impl<C> Default for Registry<C> {
    fn default() -> Self {
        Self {
            ops: HashMap::new(),
        }
    }
}

impl<C> Registry<C> {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an operation under its kind.
    ///
    /// # Panics
    ///
    /// Panics if the kind is already registered.
    pub fn register<O: Operation<C>>(&mut self, op: O) -> &mut Self {
        let prev = self.ops.insert(O::KIND, Box::new(op));
        assert!(prev.is_none(), "kind {} registered twice", O::KIND);
        self
    }

    fn get(&self, kind: &str) -> Result<&dyn Erased<C>, Unparsed> {
        self.ops
            .get(kind)
            .map(|b| b.as_ref())
            .ok_or(Unparsed::UnknownKind)
    }

    /// Whether the kind is registered and the plan parses, without running anything.
    pub(crate) fn parse(&self, p: &Planned) -> Result<(), Unparsed> {
        self.get(&p.kind)?.parse(&p.plan)
    }

    pub(crate) fn check(&self, ctx: &C, p: &Planned) -> Result<Result<(), Skip>, Unparsed> {
        self.get(&p.kind)?.check(ctx, &p.plan)
    }

    pub(crate) fn apply(&self, ctx: &C, p: &Planned) -> Applying {
        self.get(&p.kind)?.apply(ctx, &p.plan)
    }

    pub(crate) fn inverse(&self, kind: &str, done: &Value) -> Inverting {
        self.get(kind)?.inverse(done)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use serde::{Deserialize, Serialize};
    use serde_json::json;

    use super::*;
    use crate::Skip;

    type Ctx = Mutex<HashMap<String, i64>>;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct AddPlan {
        key: String,
        by: i64,
    }

    struct Add;

    impl OpKind for Add {
        const KIND: &'static str = "test.add";
        type Plan = AddPlan;
        type Done = AddPlan;
    }

    impl Operation<Ctx> for Add {
        fn check(&self, _ctx: &Ctx, plan: &AddPlan) -> Result<(), Skip> {
            if plan.by == 0 {
                Err(Skip::PreconditionFailed)
            } else {
                Ok(())
            }
        }
        fn apply(&self, ctx: &Ctx, plan: &AddPlan) -> Result<Applied<AddPlan>, Skip> {
            *ctx.lock().unwrap().entry(plan.key.clone()).or_insert(0) += plan.by;
            Ok(Applied::new(plan.clone()))
        }
        fn inverse(&self, done: &AddPlan) -> Option<AddPlan> {
            Some(AddPlan {
                key: done.key.clone(),
                by: -done.by,
            })
        }
    }

    struct Boom;

    impl OpKind for Boom {
        const KIND: &'static str = "test.boom";
        type Plan = ();
        type Done = ();
    }

    impl Operation<Ctx> for Boom {
        fn apply(&self, _ctx: &Ctx, _plan: &()) -> Result<Applied<()>, Skip> {
            panic!("boom")
        }
        fn inverse(&self, _done: &()) -> Option<()> {
            None
        }
    }

    struct BoomInverse;

    impl OpKind for BoomInverse {
        const KIND: &'static str = "test.boom_inverse";
        type Plan = ();
        type Done = ();
    }

    impl Operation<Ctx> for BoomInverse {
        fn apply(&self, _ctx: &Ctx, _plan: &()) -> Result<Applied<()>, Skip> {
            Ok(Applied::new(()))
        }
        fn inverse(&self, _done: &()) -> Option<()> {
            panic!("boom")
        }
    }

    fn registry() -> Registry<Ctx> {
        let mut r = Registry::new();
        r.register(Add).register(Boom).register(BoomInverse);
        r
    }

    fn add(key: &str, by: i64) -> Planned {
        Planned::new::<Add>(&AddPlan {
            key: key.into(),
            by,
        })
    }

    #[test]
    fn apply_runs_a_registered_kind() {
        let ctx = Ctx::default();
        let (done, children) = registry().apply(&ctx, &add("a", 2)).unwrap().unwrap();
        assert!(children.is_empty());
        assert_eq!(ctx.lock().unwrap()["a"], 2);
        assert_eq!(
            serde_json::from_value::<AddPlan>(done).unwrap(),
            AddPlan {
                key: "a".into(),
                by: 2
            }
        );
    }

    #[test]
    fn unknown_kinds_and_bad_plans_are_unparsed() {
        let ctx = Ctx::default();
        let r = registry();
        let nope = Planned {
            kind: "nope".into(),
            plan: json!(null),
        };
        assert!(matches!(r.apply(&ctx, &nope), Err(Unparsed::UnknownKind)));
        let bad = Planned {
            kind: "test.add".into(),
            plan: json!(3),
        };
        assert!(matches!(r.apply(&ctx, &bad), Err(Unparsed::InvalidPlan(_))));
    }

    #[test]
    fn check_reports_its_skip() {
        let ctx = Ctx::default();
        assert!(matches!(
            registry().check(&ctx, &add("a", 0)),
            Ok(Err(Skip::PreconditionFailed))
        ));
        assert!(matches!(registry().check(&ctx, &add("a", 1)), Ok(Ok(()))));
    }

    #[test]
    fn inverse_maps_done_to_a_plan() {
        let ctx = Ctx::default();
        let r = registry();
        let (done, _) = r.apply(&ctx, &add("a", 2)).unwrap().unwrap();
        let plan = r.inverse("test.add", &done).unwrap().unwrap().unwrap();
        let undo = Planned {
            kind: "test.add".into(),
            plan,
        };
        r.apply(&ctx, &undo).unwrap().unwrap();
        assert_eq!(ctx.lock().unwrap()["a"], 0);
    }

    #[test]
    fn a_panic_is_a_skip() {
        let ctx = Ctx::default();
        let p = Planned::new::<Boom>(&());
        match registry().apply(&ctx, &p) {
            Ok(Err(Skip::Io(m))) => assert_eq!(m, "the operation panicked"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn a_panicking_inverse_is_a_skip() {
        match registry().inverse("test.boom_inverse", &json!(null)) {
            Ok(Err(Skip::Io(m))) => assert_eq!(m, "the operation panicked"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn registering_twice_panics() {
        let mut r = Registry::<Ctx>::new();
        r.register(Add).register(Add);
    }
}
