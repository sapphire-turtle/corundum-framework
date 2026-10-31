//! How [`ServicePage`](crate::first_run::ServicePage) and
//! [`ServiceRow`](crate::settings::ServiceRow) reach the OS's service manager: through
//! [`ServiceOps`], on a [`Job`]'s thread, as a call to the manager can take seconds (a
//! `systemctl` waiting on D-Bus, a PowerShell starting) and a frame must not wait for it.

use std::sync::mpsc;

use corundum_framework_service::{Environment, ServiceSpec, ServiceState, SystemManager};

/// What the page and the rows do to a service, as plain functions of its spec: the real
/// ones ([`ServiceOps::SYSTEM`]) on this machine's service manager, stand-ins in the tests.
/// Each error is already the sentence to show.
#[derive(Clone, Copy)]
pub(crate) struct ServiceOps {
    /// What the service is doing now.
    pub(crate) state: fn(&ServiceSpec) -> Result<ServiceState, String>,
    /// Register the service and start it.
    pub(crate) install: fn(&ServiceSpec) -> Result<(), String>,
    /// Stop the service and take its registration away.
    pub(crate) uninstall: fn(&ServiceSpec) -> Result<(), String>,
    /// Stop the service, leaving it registered.
    pub(crate) stop: fn(&ServiceSpec) -> Result<(), String>,
    /// Start the registered service.
    pub(crate) start: fn(&ServiceSpec) -> Result<(), String>,
}

impl ServiceOps {
    /// The service manager of the machine the app runs on.
    pub(crate) const SYSTEM: ServiceOps = ServiceOps {
        state: system_state,
        install: system_install,
        uninstall: system_uninstall,
        stop: system_stop,
        start: system_start,
    };
}

fn system_state(spec: &ServiceSpec) -> Result<ServiceState, String> {
    corundum_framework_service::state(spec, &Environment::detect(), &SystemManager)
        .map_err(|err| err.to_string())
}

fn system_install(spec: &ServiceSpec) -> Result<(), String> {
    corundum_framework_service::install(spec, &Environment::detect(), &SystemManager)
        .map(drop)
        .map_err(|err| err.to_string())
}

fn system_uninstall(spec: &ServiceSpec) -> Result<(), String> {
    corundum_framework_service::uninstall(spec, &Environment::detect(), &SystemManager)
        .map_err(|err| err.to_string())
}

fn system_stop(spec: &ServiceSpec) -> Result<(), String> {
    corundum_framework_service::stop(spec, &Environment::detect(), &SystemManager)
        .map_err(|err| err.to_string())
}

fn system_start(spec: &ServiceSpec) -> Result<(), String> {
    corundum_framework_service::start(spec, &Environment::detect(), &SystemManager)
        .map_err(|err| err.to_string())
}

/// Held by a [`Job`]'s thread: on drop — the work's end or its panic — it lets go of the
/// answer's sender first and then repaints, so the frame it asks for sees the answer, or the
/// disconnected channel that says there is none.
struct RepaintOnDrop<T> {
    ctx: egui::Context,
    reply: Option<mpsc::Sender<Result<T, String>>>,
}

impl<T> Drop for RepaintOnDrop<T> {
    fn drop(&mut self) {
        drop(self.reply.take());
        self.ctx.request_repaint();
    }
}

/// Work running on a thread of its own, whose answer the UI picks up on a later frame.
pub(crate) struct Job<T> {
    answer: mpsc::Receiver<Result<T, String>>,
}

impl<T: Send + 'static> Job<T> {
    /// Run `work` on a thread named `name`; its answer repaints `ctx`. A thread that cannot
    /// be started answers at once with why.
    pub(crate) fn spawn(
        ctx: &egui::Context,
        name: &str,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Job<T> {
        let (sender, answer) = mpsc::channel();
        let ctx = ctx.clone();
        let reply = sender.clone();
        let spawned = std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                // Repaints however the work ends, a panic included, so the frame that polls
                // finds the answer, or that there is none.
                let guard = RepaintOnDrop {
                    ctx,
                    reply: Some(reply),
                };
                let answer = work();
                if let Some(reply) = &guard.reply {
                    let _ = reply.send(Ok(answer));
                }
            });
        if let Err(err) = spawned {
            tracing::warn!("cannot start the {name} thread: {err}");
            let _ = sender.send(Err(err.to_string()));
        }
        Job { answer }
    }

    /// The answer, once there is one: the work's, or why there is none (the thread could
    /// not start, or panicked).
    pub(crate) fn poll(&self) -> Option<Result<T, String>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(answer),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "the background task stopped without an answer".to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn a_panicking_job_still_repaints_with_its_answer_ready() {
        let ctx = egui::Context::default();
        let repainted = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&repainted);
        ctx.set_request_repaint_callback(move |_| flag.store(true, Ordering::SeqCst));
        let job = Job::spawn(&ctx, "panicking job", || -> () { panic!("on purpose") });
        let deadline = Instant::now() + Duration::from_secs(5);
        while !repainted.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(repainted.load(Ordering::SeqCst), "no repaint was requested");
        // The repaint comes after the answer: the frame it asks for finds why there is none.
        assert!(matches!(job.poll(), Some(Err(_))));
    }
}
