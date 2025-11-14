use std::sync::Arc;

use tokio::{
    sync::{
        Mutex, RwLock,
        mpsc::{Receiver, Sender},
    },
    task::JoinHandle,
};
use tokio_cron_scheduler::{Job, JobScheduler as TCScheduler};
use tracing::{error, info};

use crate::{
    channels::{JobSchedulerMessages, RuntimeMessages},
    plugins::{PluginRegistrationRequestsScheduledJob, PluginRegistrations},
};

pub struct JobScheduler {
    tokio_cron_scheduler: Arc<RwLock<TCScheduler>>,
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
                tokio_cron_scheduler: Arc::new(RwLock::new(job_scheduler)),
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
        initialized_plugin_registrations_scheduled_jobs: Vec<
            PluginRegistrationRequestsScheduledJob,
        >,
    ) {
        for scheduled_job in &initialized_plugin_registrations_scheduled_jobs {
            for cron in &scheduled_job.crons {
                info!(
                    "Scheduled Job {} from the {} plugin requested to be registered.",
                    &scheduled_job.id, &scheduled_job.plugin_id
                );

                let runtime_tx = self.runtime_tx.clone();
                let plugin_id = scheduled_job.plugin_id.clone();
                let internal_id = scheduled_job.id.clone();

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
                            &scheduled_job.id, &scheduled_job.plugin_id, &err
                        );
                        continue;
                    }
                };

                match self.tokio_cron_scheduler.read().await.add(job).await {
                    Ok(uuid) => {
                        self.plugin_registrations
                            .write()
                            .await
                            .scheduled_jobs
                            .insert(
                                uuid.as_u128(),
                                (scheduled_job.plugin_id.clone(), scheduled_job.id.clone()),
                            );
                    }
                    Err(err) => {
                        error!(
                            "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                            &scheduled_job.id, &scheduled_job.plugin_id, &err
                        );
                    }
                }
            }
        }
    }

    pub async fn start(self) -> Result<JoinHandle<()>, ()> {
        if let Err(err) = self.tokio_cron_scheduler.read().await.start().await {
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
                        let _ = job_scheduler
                            .tokio_cron_scheduler
                            .write()
                            .await
                            .shutdown()
                            .await;
                        let _ = is_done.send(());
                    }
                }
            }
        }))
    }
}
