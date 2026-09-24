//! A single shared tokio runtime. GTK code spawns futures here and awaits the returned
//! `JoinHandle` inside `glib::spawn_future_local`, so the main thread never blocks.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .thread_name("primvokon-rt")
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

/// Spawn a future on the shared runtime.
pub fn spawn<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(fut)
}

/// Run a future to completion from a non-async context (CLI helpers and tests only).
pub fn block_on<F: Future>(fut: F) -> F::Output {
    runtime().block_on(fut)
}

/// Handle to the runtime for libraries that need one explicitly.
pub fn handle() -> tokio::runtime::Handle {
    runtime().handle().clone()
}
