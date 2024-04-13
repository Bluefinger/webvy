use std::{future::Future, sync::{atomic::AtomicU32, Arc}};

use bevy_ecs::system::{CommandQueue, Resource};
use bevy_tasks::{IoTaskPool, Task};
use event_listener::{Event, IntoNotification};
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
        I: FnOnce(DeferredScope) -> F + Send + 'static,
    {
        let channel = self.channel.clone();
        let guard = DeferredGuard::new(self);

        IoTaskPool::get().spawn(async move {
            let result = task(DeferredScope::new(channel)).await;

            drop(guard);

            result
        })
    }

    pub fn scoped_task_local<T: 'static, I, F>(&self, task: I) -> Task<T>
    where
        F: Future<Output = T> + 'static,
        I: FnOnce(DeferredScope) -> F + 'static,
    {
        let channel = self.channel.clone();
        let guard = DeferredGuard::new(self);

        IoTaskPool::get().spawn_local(async move {
            let result = task(DeferredScope::new(channel)).await;

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
        self.waiting.fetch_sub(1, std::sync::atomic::Ordering::Release);
        self.finished.notify(1.relaxed());
    }
}

#[derive(Debug)]
pub struct DeferredScope {
    channel: Sender<CommandQueue>,
}

impl DeferredScope {
    fn new(channel: Sender<CommandQueue>) -> Self {
        Self { channel }
    }

    pub fn send(&self, msg: CommandQueue) {
        self.channel
            .try_send(msg)
            .expect("Deferred channel should always be open and never full");
    }

    pub fn spawn<T: Send + 'static>(
        &self,
        task: impl Future<Output = T> + Send + 'static,
    ) -> Task<T> {
        IoTaskPool::get().spawn(task)
    }
}

impl AsRef<DeferredScope> for DeferredScope {
    fn as_ref(&self) -> &DeferredScope {
        self
    }
}
