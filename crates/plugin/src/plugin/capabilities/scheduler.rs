use crate::plugin::{PluginRuntime, guest_call};
use crate::wit::exports::thrustr::plugin::scheduler;
use anyhow::Context;
use async_trait::async_trait;
use domain::component::{Error, PeriodicTask, Scheduler};
use std::{sync::Arc, time::Duration};

pub struct PluginScheduler {
    runtime: Arc<PluginRuntime>,
    indices: scheduler::GuestIndices,
    periodic: Vec<PeriodicTask>,
}

impl PluginScheduler {
    pub async fn resolve(runtime: &Arc<PluginRuntime>) -> anyhow::Result<Option<Self>> {
        let Some(indices) = runtime.export("scheduler", scheduler::GuestIndices::new)? else {
            return Ok(None);
        };

        let periodic = guest_call!(runtime, indices, |scheduler, accessor| async {
            scheduler.call_periodic(accessor).await.map(Ok)
        })
        .context("reading the plugin's periodic tasks failed")?;

        Ok(Some(Self {
            runtime: runtime.clone(),
            indices,
            periodic: periodic
                .into_iter()
                .map(|periodic| PeriodicTask {
                    task: periodic.task,
                    interval: Duration::from_millis(periodic.interval_ms),
                })
                .collect(),
        }))
    }
}

#[async_trait]
impl Scheduler for PluginScheduler {
    fn periodic(&self) -> &[PeriodicTask] {
        &self.periodic
    }

    async fn run(&self, task: &str, args: &[u8]) -> Result<(), Error> {
        let (task, args) = (task.to_owned(), args.to_vec());
        guest_call!(self.runtime, self.indices, |scheduler, accessor| {
            scheduler.call_run(accessor, task, args)
        })
    }
}
