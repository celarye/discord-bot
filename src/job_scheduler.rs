use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};
use tokio_cron_scheduler::{Job, JobScheduler as InternalJobScheduler};
use tracing::error;

use crate::{
    discord::data::Data,
    plugins::{InitializedPlugins, Runtime},
};

pub struct JobScheduler {
    job_scheduler: InternalJobScheduler,
    runtime: Arc<Mutex<Runtime>>,
    data: Arc<RwLock<Box<Data>>>,
}

impl JobScheduler {
    pub async fn new(
        runtime: Arc<Mutex<Runtime>>,
        initialized_plugins: Arc<RwLock<InitializedPlugins>>,
        data: Arc<RwLock<Box<Data>>>,
    ) -> Result<Self, ()> {
        let job_scheduler = match InternalJobScheduler::new().await {
            Ok(job_scheduler) => job_scheduler,
            Err(err) => {
                error!(
                    "Something went wrong while creating a new instance of the job scheduler, error {}",
                    &err
                );
                return Err(());
            }
        };

        for scheduled_task in &initialized_plugins.read().await.scheduled_tasks {
            for scheduled_task_entry in scheduled_task.1 {
                let runtime = runtime.clone();
                let data = data.clone();

                let plugin_name = scheduled_task_entry.0.clone();
                let scheduled_task_name = scheduled_task_entry.1.clone();

                let job = match Job::new_async_tz(
                    scheduled_task.0.clone(),
                    chrono::Local,
                    move |_uuid, _lock| {
                        let runtime = runtime.clone();
                        let data = data.clone();

                        let plugin_name = plugin_name.clone();
                        let scheduled_task_name = scheduled_task_name.clone();

                        Box::pin(async move {
                            runtime
                                .lock()
                                .await
                                .call_scheduled_task(
                                    &plugin_name,
                                    &scheduled_task_name,
                                    data.clone(),
                                )
                                .await;
                        })
                    },
                ) {
                    Ok(job) => job,
                    Err(err) => {
                        error!(
                            "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                            &scheduled_task_entry.1, &scheduled_task_entry.0, &err
                        );
                        continue;
                    }
                };

                if let Err(err) = job_scheduler.add(job).await {
                    error!(
                        "Something went wrong while adding {} job from the {} plugin to the job scheduler, error: {}",
                        &scheduled_task_entry.1, &scheduled_task_entry.0, &err
                    );
                }
            }
        }

        Ok(JobScheduler {
            job_scheduler,
            runtime: runtime.clone(),
            data: data.clone(),
        })
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
                    .lock()
                    .await
                    .call_scheduled_task(&plugin_name, &scheduled_job_name, data.clone())
                    .await;
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

    pub async fn start(&self) -> Result<(), ()> {
        if let Err(err) = self.job_scheduler.start().await {
            error!(
                "Something went wrong while starting the job scheduler, error: {}",
                &err
            );
            return Err(());
        }

        Ok(())
    }
}
