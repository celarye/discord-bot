use tokio::task::JoinHandle;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::error;

use crate::discord::DiscordBotClient;

impl DiscordBotClient {
    pub async fn new_job_scheduler() -> Result<JobScheduler, ()> {
        match JobScheduler::new().await {
            Ok(job_scheduler) => Ok(job_scheduler),
            Err(err) => {
                error!(
                    "Something went wrong while creating a new instance of the job scheduler, error {}",
                    &err
                );
                Err(())
            }
        }
    }

    // TODO: Switch to register struct
    pub async fn register_scheduled_jobs(&self) {
        for scheduled_job in self
            .data
            .initialized_plugins
            .read()
            .await
            .scheduled_jobs
            .iter()
        {
            for scheduled_job_entry in scheduled_job.1 {
                let runtime = self.runtime.clone();
                let data = self.data.clone();

                let plugin_name = scheduled_job_entry.0.clone();
                let scheduled_job_name = scheduled_job_entry.1.clone();

                let job = match Job::new_async_tz(
                    scheduled_job.0.clone(),
                    chrono::Local,
                    move |_uuid, _lock| {
                        let runtime = runtime.clone();
                        let data = data.clone();

                        let plugin_name = plugin_name.clone();
                        let scheduled_job_name = scheduled_job_name.clone();

                        Box::pin(async move {
                            runtime
                                .call_scheduled_job(&plugin_name, &scheduled_job_name)
                                .await;

                            // TODO: Handler function
                        })
                    },
                ) {
                    Ok(job) => job,
                    Err(err) => {
                        error!(
                            "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                            &scheduled_job_entry.1, &scheduled_job_entry.0, &err
                        );
                        continue;
                    }
                };

                if let Err(err) = self.job_scheduler.add(job).await {
                    error!(
                        "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                        &scheduled_job_entry.1, &scheduled_job_entry.0, &err
                    );
                }
            }
        }
    }

    pub async fn add(
        &self,
        plugin_name: String,
        scheduled_job_name: String,
        cron: &str,
    ) -> Result<(), String> {
        let runtime = self.runtime.clone();
        let data = self.data.clone();

        let job_plugin_name = plugin_name.clone();
        let job_scheduled_job_name = scheduled_job_name.clone();

        let job = match Job::new_async_tz(cron, chrono::Local, move |_uuid, _lock| {
            let runtime = runtime.clone();
            let data = data.clone();

            let plugin_name = job_plugin_name.clone();
            let scheduled_job_name = job_scheduled_job_name.clone();

            Box::pin(async move {
                runtime
                    .call_scheduled_job(&plugin_name, &scheduled_job_name)
                    .await;

                // TODO: Handler function
            })
        }) {
            Ok(job) => job,
            Err(err) => {
                let error = format!(
                    "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                    scheduled_job_name, plugin_name, &err
                );

                error!(error);

                return Err(error);
            }
        };

        if let Err(err) = self.job_scheduler.add(job).await {
            let error = format!(
                "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                scheduled_job_name, plugin_name, &err
            );

            error!(error);

            return Err(error);
        }

        Ok(())
    }

    pub async fn remove(
        &self,
        plugin_name: &str,
        scheduled_job_name: &str,
        cron: &str,
    ) -> Result<(), String> {
        // TODO: Store UUID in initialized_plugins for easy access
        //self.job_scheduler.remove(to_be_removed);
        unimplemented!()
    }

    pub async fn start_job_scheduler(&self, tasks: &mut Vec<JoinHandle<()>>) -> Result<(), ()> {
        if let Err(err) = self.job_scheduler.start().await {
            error!(
                "Something went wrong while starting the job scheduler, error: {}",
                &err
            );
            return Err(());
        }

        Ok(())
    }

    pub async fn shutdown_job_scheduler(&mut self) {
        let _ = self.job_scheduler.shutdown().await;
    }
}
