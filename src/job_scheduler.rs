use std::sync::Arc;

use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler as TCScheduler};
use tracing::error;

use crate::{
    discord::DiscordBotClientSender,
    plugins::{PluginRegistrationRequestsScheduledJob, PluginRegistrations, runtime::Runtime},
};

pub struct JobScheduler {
    job_scheduler: RwLock<TCScheduler>,
    pub runtime: Arc<Runtime>,
    plugin_registrations: Arc<RwLock<PluginRegistrations>>,
}

impl JobScheduler {
    pub async fn new(
        discord_bot_client_sender: Arc<DiscordBotClientSender>,
        plugin_registrations: Arc<RwLock<PluginRegistrations>>,
    ) -> Result<Arc<Self>, ()> {
        match TCScheduler::new().await {
            Ok(tcscheduler) => Ok(Arc::new_cyclic(|job_scheduler| {
                let runtime = Arc::new(Runtime::new(
                    discord_bot_client_sender,
                    job_scheduler.clone(),
                ));

                JobScheduler {
                    job_scheduler: RwLock::new(tcscheduler),
                    runtime,
                    plugin_registrations,
                }
            })),
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
        for scheduled_job in initialized_plugin_registrations_scheduled_jobs {
            let runtime = self.runtime.clone();
            let plugin_id = scheduled_job.plugin_id.clone();
            let internal_id = scheduled_job.internal_id.clone();

            let job = match Job::new_async_tz(
                scheduled_job.cron.clone(),
                chrono::Local,
                move |_uuid, _lock| {
                    let runtime = runtime.clone();
                    let plugin_id = plugin_id.clone();
                    let internal_id = internal_id.clone();

                    Box::pin(async move {
                        runtime.call_scheduled_job(&plugin_id, &internal_id).await;
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

            match self.job_scheduler.write().await.add(job).await {
                Ok(uuid) => {
                    self.plugin_registrations
                        .write()
                        .await
                        .scheduled_jobs
                        .insert(
                            uuid.as_u128(),
                            (scheduled_job.plugin_id, scheduled_job.internal_id),
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

    pub async fn start(&self) -> Result<(), ()> {
        if let Err(err) = self.job_scheduler.read().await.start().await {
            error!(
                "Something went wrong while starting the job scheduler, error: {}",
                &err
            );
            return Err(());
        }

        Ok(())
    }

    pub async fn shutdown(&self) {
        let _ = self.job_scheduler.write().await.shutdown().await;
    }
}
