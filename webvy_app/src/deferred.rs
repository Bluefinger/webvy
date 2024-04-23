use std::{
    future::Future,
    rc::Rc,
    sync::{atomic::AtomicU32, Arc},
};

use bevy_ecs::system::{CommandQueue, Resource};
use bevy_tasks::{IoTaskPool, Task};
use event_listener::{Event, IntoNotification};
use log::trace;
use smol::channel::Sender;

#[derive(Debug, Resource)]
pub struct DeferredTask {
    channel: Sender<CommandQueue>,
    finished: Arc<Event>,
    waiting: Arc<AtomicU32>,
}

impl DeferredTask {
    pub(crate) fn new(channel: Sender<CommandQueue>, finished: Arc<Event>) -> Self {
        Self {
            channel,
            finished,
            waiting: Arc::new(AtomicU32::new(0)),
        }
    }

    pub(crate) fn waiting(&self) -> u32 {
        self.waiting.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn scoped_task<T: Send + 'static, I, F>(&self, task: I) -> Task<T>
    where
        F: Future<Output = T> + Send + 'static,
        I: FnOnce(Arc<DeferredScope>) -> F + Send + 'static,
    {
        let guard = DeferredGuard::new(self);
        let scope = Arc::new(DeferredScope::new(self));

        IoTaskPool::get().spawn(async move {
            let result = task(scope.clone()).await;

            drop(guard);

            result
        })
    }

    pub fn scoped_task_local<T: 'static, I, F>(&self, task: I) -> Task<T>
    where
        F: Future<Output = T> + 'static,
        I: FnOnce(Rc<DeferredScope>) -> F + 'static,
    {
        let guard = DeferredGuard::new(self);
        let scope = Rc::new(DeferredScope::new(self));

        IoTaskPool::get().spawn_local(async move {
            let result = task(scope.clone()).await;

            drop(guard);

            result
        })
    }
}

#[derive(Debug)]
struct DeferredGuard {
    finished: Arc<Event>,
    waiting: Arc<AtomicU32>,
}

impl DeferredGuard {
    fn new(deferred: &DeferredTask) -> Self {
        let finished = deferred.finished.clone();
        let waiting = deferred.waiting.clone();

        waiting.fetch_add(1, std::sync::atomic::Ordering::Release);

        Self { finished, waiting }
    }
}

impl Drop for DeferredGuard {
    fn drop(&mut self) {
        self.waiting
            .fetch_sub(1, std::sync::atomic::Ordering::Release);
        trace!("{} listeners", self.finished.total_listeners());
        self.finished.notify(1.relaxed());
        trace!("1 listener notified: {}", self.finished.is_notified());
    }
}

#[derive(Debug)]
pub struct DeferredScope {
    channel: Sender<CommandQueue>,
}

impl DeferredScope {
    fn new(deferred: &DeferredTask) -> Self {
        let channel = deferred.channel.clone();

        Self { channel }
    }

    pub fn send(&self, msg: CommandQueue) {
        self.channel
            .try_send(msg)
            .expect("Deferred channel should always be open and never full");
    }

    pub fn spawn<S: Send + 'static>(
        &self,
        task: impl Future<Output = S> + Send + 'static,
    ) -> Task<S> {
        IoTaskPool::get().spawn(task)
    }

    pub fn spawn_local<S: 'static>(
        &self,
        task: impl Future<Output = S> + 'static,
    ) -> Task<S> {
        IoTaskPool::get().spawn_local(task)
    }
}
