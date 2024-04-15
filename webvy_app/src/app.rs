use std::sync::Arc;

use bevy_ecs::{
    schedule::{ExecutorKind, InternedScheduleLabel, IntoSystemConfigs, Schedule, ScheduleLabel},
    system::{CommandQueue, Resource},
    world::World,
};
use bevy_tasks::{ComputeTaskPool, IoTaskPool, TaskPoolBuilder};
use event_listener::{Event, Listener};
use log::trace;
use smol::channel::{unbounded, Receiver};

use crate::{deferred::DeferredTask, traits::ProcessorPlugin};

pub struct ProcessorApp {
    world: World,
    schedules: Vec<InternedScheduleLabel>,
    deferred: Receiver<CommandQueue>,
    finished: Arc<Event>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct Preload;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct Load;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct Process;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct PostProcess;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
pub struct Write;

impl ProcessorApp {
    pub fn new() -> Self {
        setup_threadpool();
        let (sender, deferred) = unbounded();
        let finished = Arc::new(Event::new());

        let mut world = World::new();

        world.insert_resource(DeferredTask::new(sender, finished.clone()));

        let (world, schedules) = Self::init_schedules(world);

        Self {
            world,
            schedules,
            deferred,
            finished,
        }
    }

    fn init_schedules(mut world: World) -> (World, Vec<InternedScheduleLabel>) {
        // Preload/Load schedules should be mostly IO focused, so most of
        // the needed concurrency should be occurring on the IO executor.
        // Therefore there's little need to MT loading systems.
        let mut preload = Schedule::new(Preload);
        preload.set_executor_kind(ExecutorKind::SingleThreaded);

        let mut load = Schedule::new(Load);
        load.set_executor_kind(ExecutorKind::SingleThreaded);

        // Heavy CPU processing should be happening here with little if any
        // IO occuring.
        let mut process = Schedule::new(Process);
        process.set_executor_kind(ExecutorKind::MultiThreaded);

        // Heavy CPU processing should be happening here with little if any
        // IO occuring.
        let mut postprocess = Schedule::new(PostProcess);
        postprocess.set_executor_kind(ExecutorKind::MultiThreaded);

        // Write schedule should be more focused around spawning io tasks
        // for outputting the results to disk storage. Therefore there's
        // little need to MT the systems here as the concurrency will occur
        // on the IO executor instead.
        let mut write = Schedule::new(Write);
        write.set_executor_kind(ExecutorKind::SingleThreaded);

        let schedules = vec![
            preload.label(),
            load.label(),
            process.label(),
            postprocess.label(),
            write.label(),
        ];

        world.add_schedule(preload);
        world.add_schedule(load);
        world.add_schedule(process);
        world.add_schedule(postprocess);
        world.add_schedule(write);

        (world, schedules)
    }

    pub fn insert_resource<R: Resource>(&mut self, value: R) -> &mut Self {
        self.world.insert_resource(value);

        self
    }

    pub fn init_resource<R: Resource + Default>(&mut self) -> &mut Self {
        self.world.init_resource::<R>();

        self
    }

    pub fn add_systems<M>(
        &mut self,
        label: impl ScheduleLabel,
        systems: impl IntoSystemConfigs<M>,
    ) -> &mut Self {
        self.world.schedule_scope(label, |_, schedule| {
            schedule.add_systems(systems);
        });

        self
    }

    pub fn add_processor(&mut self, plugin: impl ProcessorPlugin) -> &mut Self {
        plugin.register(self);

        self
    }

    pub fn run(&mut self) {
        let compute = ComputeTaskPool::get();
        let io = IoTaskPool::get();
        let schedules = self.schedules.iter();

        for &schedule in schedules {
            trace!(target: "executor", "Running schedule: {:?}", schedule);
            self.world.run_schedule(schedule);

            // Local tasks for the schedule MUST be exhausted before we can proceed.
            compute.with_local_executor(|cex| while cex.try_tick() {});

            // Remaining tasks on other threads
            let deferred_actions = self.world.resource::<DeferredTask>().waiting();

            trace!(target: "executor", "Waiting on: {} actions", deferred_actions);

            if deferred_actions > 0 {
                trace!(target: "executor", "Waiting for async processes to finish");

                for _ in 0..deferred_actions {
                    trace!(target: "executor", "Listening for a notification");
                    loop {
                        let listener = self.finished.listen();

                        // Tick the local executor in case we are waiting for something there
                        io.with_local_executor(|iex| while iex.try_tick() {});

                        // Timeout so we can yield the main thread for ticking the local executor in case the task
                        // is delayed there.
                        if listener
                            .wait_timeout(std::time::Duration::from_millis(100))
                            .is_some()
                        {
                            trace!(target: "executor", "Received notification! Deferred task finished");
                            break;
                        }
                    }
                }

                trace!(target: "executor", "All async processes finished!");

                trace!(target: "executor", "Apply queued deferred commands before proceeding with next schedule");
                let mut deferred_queue = CommandQueue::default();
                while let Ok(mut commands) = self.deferred.try_recv() {
                    deferred_queue.append(&mut commands);
                }
                deferred_queue.apply(&mut self.world);
            }
        }
    }
}

impl Default for ProcessorApp {
    fn default() -> Self {
        Self::new()
    }
}

fn setup_threadpool() {
    let threads = bevy_tasks::available_parallelism();

    let compute_threads = threads.div_ceil(2).saturating_sub(1).max(1);
    let io_threads = threads.div_ceil(4).min(6);

    let compute = ComputeTaskPool::get_or_init(|| {
        TaskPoolBuilder::default()
            .num_threads(compute_threads)
            .thread_name("Compute Task Pool".to_string())
            .build()
    });

    trace!("Initialised {} compute threads", compute.thread_num());

    let io = IoTaskPool::get_or_init(|| {
        TaskPoolBuilder::default()
            .num_threads(io_threads)
            .thread_name("IO Task Pool".to_string())
            .build()
    });

    trace!("Initialised {} io threads", io.thread_num());
}
