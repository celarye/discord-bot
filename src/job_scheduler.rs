use std::sync::Arc;

use futures::executor::block_on;
use tokio::{
    sync::{
        Mutex, RwLock,
        mpsc::{Receiver, Sender},
    },
    task::JoinHandle,
};
use tokio_cron_scheduler::{Job, JobScheduler as TCScheduler};
use tracing::error;

use crate::{
    channels::{JobSchedulerMessages, RuntimeMessages},
    plugins::{PluginRegistrations, ScheduledJobRegistrationRequest},
};

pub struct JobScheduler {
    job_scheduler: Arc<RwLock<TCScheduler>>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    runtime_tx: Arc<Sender<RuntimeMessages>>,
    runtime_rx: Arc<Mutex<Receiver<JobSchedulerMessages>>>,
}

impl JobScheduler {
    pub async fn new(
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
        runtime_tx: Sender<RuntimeMessages>,
        runtime_rx: Receiver<JobSchedulerMessages>,
    ) -> Result<Self, ()> {
        match TCScheduler::new().await {
            Ok(job_scheduler) => Ok(JobScheduler {
                job_scheduler: Arc::new(RwLock::new(job_scheduler)),
                plugin_registrations,
                runtime_tx: Arc::new(runtime_tx),
                runtime_rx: Arc::new(Mutex::new(runtime_rx)),
            }),
            Err(err) => {
                error!(
                    "Something went wrong while creating a new instance of the job scheduler, error {}",
                    &err
                );
                Err(())
            }
        }
    }

    pub async fn scheduled_job_registrations(
        &self,
        initialized_plugin_registrations_scheduled_jobs: Vec<ScheduledJobRegistrationRequest>,
    ) {
        for scheduled_job in initialized_plugin_registrations_scheduled_jobs.iter() {
            for cron in scheduled_job.crons.iter() {
                let runtime_tx = self.runtime_tx.clone();
                let plugin_id = scheduled_job.plugin_id.clone();
                let internal_id = scheduled_job.internal_id.clone();

                let job = match Job::new_async_tz(
                    cron.clone(),
                    chrono::Local,
                    move |_uuid, _lock| {
                        let runtime_tx = runtime_tx.clone();
                        let plugin_id = plugin_id.clone();
                        let internal_id = internal_id.clone();

                        Box::pin(async move {
                            let _ = runtime_tx
                                .send(RuntimeMessages::CallScheduledJob(plugin_id, internal_id))
                                .await;
                        })
                    },
                ) {
                    Ok(job) => job,
                    Err(err) => {
                        error!(
                            "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                            &scheduled_job.internal_id, &scheduled_job.plugin_id, &err
                        );
                        continue;
                    }
                };

                match self.job_scheduler.read().await.add(job).await {
                    Ok(uuid) => {
                        self.plugin_registrations
                            .write()
                            .await
                            .scheduled_jobs
                            .insert(
                                uuid.as_u128(),
                                (
                                    scheduled_job.plugin_id.clone(),
                                    scheduled_job.internal_id.clone(),
                                ),
                            );
                    }
                    Err(err) => {
                        error!(
                            "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                            &scheduled_job.internal_id, &scheduled_job.plugin_id, &err
                        );
                    }
                }
            }
        }
    }

    pub async fn start(self) -> Result<JoinHandle<()>, ()> {
        if let Err(err) = self.job_scheduler.read().await.start().await {
            error!(
                "Something went wrong while starting the job scheduler, error: {}",
                &err
            );
            return Err(());
        }

        let job_scheduler = Arc::new(self);

        Ok(tokio::spawn(async move {
            while let Some(message) = job_scheduler.runtime_rx.lock().await.recv().await {
                match message {
                    JobSchedulerMessages::RegisterScheduledJobs(scheduled_jobs) => {
                        job_scheduler
                            .scheduled_job_registrations(scheduled_jobs)
                            .await;
                    }
                    JobSchedulerMessages::Shutdown(is_done) => {
                        let _ = job_scheduler.job_scheduler.write().await.shutdown().await;
                        let _ = is_done.send(());
                    }
                };
            }
        }))
    }
}

// HACK: Temp fix for error logs when the job scheduler is dropped before it is started
impl Drop for JobScheduler {
    fn drop(&mut self) {
        block_on(async {
            if !self.job_scheduler.read().await.inited().await {
                _ = self.job_scheduler.read().await.start().await;
            }
            _ = self.job_scheduler.write().await.shutdown().await;
        })
    }
}
