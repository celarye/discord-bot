/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright © 2026 Eduard Smet */

use std::sync::Arc;

use anyhow::Result;
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
};
use tokio_cron_scheduler::{Job, JobScheduler as TokioCronScheduler};
use tracing::info;
use uuid::Uuid;

use crate::utils::channels::{
    CoreMessages, JobSchedulerMessages, RuntimeMessages, RuntimeMessagesJobScheduler,
};

pub struct JobScheduler {
    tokio_cron_scheduler: TokioCronScheduler,
    core_tx: Arc<Sender<CoreMessages>>,
    rx: Receiver<JobSchedulerMessages>,
}

impl JobScheduler {
    pub async fn new(
        core_tx: Sender<CoreMessages>,
        rx: Receiver<JobSchedulerMessages>,
    ) -> Result<Self> {
        info!("Creating the job scheduler");

        Ok(JobScheduler {
            tokio_cron_scheduler: TokioCronScheduler::new().await?,
            core_tx: Arc::new(core_tx),
            rx,
        })
    }

    pub async fn start(mut self) -> Result<JoinHandle<()>> {
        self.tokio_cron_scheduler.start().await?;

        Ok(tokio::spawn(async move {
            while let Some(message) = self.rx.recv().await {
                match message {
                    JobSchedulerMessages::AddJob(plugin_id, cron, result) => {
                        let _ = result.send(self.add_job(plugin_id, cron).await);
                    }
                    JobSchedulerMessages::RemoveJob(uuid, result) => {
                        let _ = result.send(self.remove_job(uuid).await);
                    }
                }
            }

            let _ = self.tokio_cron_scheduler.shutdown().await;
        }))
    }

    async fn add_job(&self, plugin_id: Uuid, cron: String) -> Result<Uuid> {
        info!(
            "Scheduled Job at {cron} cron from the {plugin_id} plugin requested to be registered"
        );

        let core_tx = self.core_tx.clone();

        let job = Job::new_async_tz(cron.clone(), chrono::Local, move |job_id, _lock| {
            let core_tx = core_tx.clone();

            Box::pin(async move {
                let _ = core_tx
                    .send(CoreMessages::Runtime(RuntimeMessages::JobScheduler(
                        RuntimeMessagesJobScheduler::CallScheduledJob(plugin_id, job_id),
                    )))
                    .await;
            })
        })?;

        Ok(self.tokio_cron_scheduler.add(job).await?)
    }

    async fn remove_job(&self, uuid: Uuid) -> Result<()> {
        info!("Removing scheduled Job {uuid}");

        Ok(self.tokio_cron_scheduler.remove(&uuid).await?)
    }
}
