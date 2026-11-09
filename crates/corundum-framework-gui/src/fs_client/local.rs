//! The methods answered in this process (`fs-local`).

use std::sync::Arc;

use corundum_framework_fs::journal::Actor;
use corundum_framework_fs::{FsService, WorkspaceApp};
use corundum_framework_platform::pending::{self, Pending};
use serde_json::Value;

use super::{FsError, FsTransport};

type Call = dyn Fn(&str, Value) -> Pending<Result<Value, FsError>> + Send + Sync;

/// An [`FsTransport`] to an `FsService` in this process: each call dispatched on a tokio
/// runtime, for `actor`.
pub struct LocalTransport {
    call: Box<Call>,
}

impl LocalTransport {
    /// Calls `service` as `actor`, on `runtime`; `wake` runs when an answer comes, to draw
    /// the window again.
    ///
    /// The calls are only spawned on `runtime`: it must run them on threads of its own (a
    /// multi-thread runtime, or one driven elsewhere), not be driven only by the UI thread,
    /// whose frames never wait for an answer.
    pub fn new<A: WorkspaceApp>(
        service: FsService<A>,
        actor: Actor,
        runtime: tokio::runtime::Handle,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> LocalTransport {
        let wake = Arc::new(wake);
        LocalTransport {
            call: Box::new(move |method, params| {
                let wake = Arc::clone(&wake);
                // `Result` has no default, so the channel's is `None`: a sender dropped
                // unanswered (a runtime shut down drops its tasks) answers
                // `FsError::default()`.
                let (answer, sender) = pending::channel(move || wake());
                let (service, actor, method) = (service.clone(), actor.clone(), method.to_owned());
                runtime.spawn(async move {
                    let result = service
                        .dispatch(&method, params, &actor)
                        .await
                        .map_err(|e| FsError::Rpc {
                            code: e.code(),
                            message: e.to_string(),
                        });
                    sender.send(Some(result));
                });
                answer.map(|answer| answer.unwrap_or_else(|| Err(FsError::default())))
            }),
        }
    }
}

impl FsTransport for LocalTransport {
    fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>> {
        (self.call)(method, params)
    }
}
