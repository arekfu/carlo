pub mod cache;

use std::cmp::Ordering;
use std::fmt;
use std::sync::mpsc::Sender;
use std::thread::sleep;
use std::time::Duration;

use reqwest::{Client, Error};

use carlo::Event;
use config::{Config, JenkinsConfig};

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct BuildNumber(pub u32);

impl fmt::Display for BuildNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct BuildDuration(pub u32);

impl fmt::Display for BuildDuration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} s", self.0 / 1000)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct BuildUrl(String);

impl fmt::Display for BuildUrl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct JBuild {
    pub result: Option<String>,
    pub timestamp: cache::Timestamp,
    pub number: BuildNumber,
    pub duration: BuildDuration,
    pub url: BuildUrl,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JJob {
    pub name: cache::Name,
    pub last_build: JBuild,
}

#[derive(Deserialize, Debug, Clone)]
struct JJobVec(Vec<JJob>);

#[derive(Deserialize, Debug, Clone)]
struct JJson {
    jobs: JJobVec,
}

#[derive(Debug)]
pub struct JListener {
    tx: Sender<Event>,
    most_recent: cache::Cache,
}

impl JListener {
    pub fn new(tx: Sender<Event>) -> JListener {
        JListener {
            tx,
            most_recent: cache::Cache::new(),
        }
    }

    fn attempt(&self, client: &Client, j_config: &JenkinsConfig) -> Result<JJson, Error> {
        info!(
            "Attempting connection to \"{}\" ({}) as {}",
            j_config.id, j_config.server, j_config.user
        );
        let mut response = client
            .get(&j_config.server)
            .basic_auth(&j_config.user, Some(&j_config.token))
            .send()?;
        response.json()
    }

    fn prune_missing_builds(&mut self, job_vec: &Vec<JJob>, j_config: &JenkinsConfig) {
        let mut build_names = Vec::new() as Vec<&cache::Name>;
        job_vec
            .iter()
            .for_each(|ref job| build_names.push(&job.name));
        info!(
            "Will keep {} builds for server {}",
            build_names.len(),
            j_config.id
        );
        self.most_recent
            .prune_except(&j_config.server, &build_names);
    }

    fn update_builds(&mut self, mut job_vec: Vec<JJob>, j_config: &JenkinsConfig) -> Vec<Event> {
        let mut events = Vec::new();
        job_vec
            .drain(..)
            .for_each(|job| match job.last_build.result {
                None => {
                    info!(
                        "Job {} has a new build, but it is not complete yet",
                        job.name
                    );
                }
                Some(result) => {
                    let new_timestamp = job.last_build.timestamp;
                    match self
                        .most_recent
                        .insert(&j_config.server, &job.name, &new_timestamp)
                    {
                        Some(old_timestamp) => match old_timestamp.cmp(&new_timestamp) {
                            Ordering::Less => {
                                info!("Job {} has a new build", job.name);
                                events.push(Event::UpdatedJob(
                                    j_config.id.clone(),
                                    job.name.clone(),
                                    result.clone(),
                                    job.last_build.number,
                                    job.last_build.duration,
                                    job.last_build.url.clone(),
                                    j_config.notify.clone(),
                                ));
                            }
                            Ordering::Equal => info!("Job {} was not updated", job.name),
                            Ordering::Greater => warn!(
                                "Job {} went back in time from timestamp {} to {}",
                                job.name, old_timestamp, new_timestamp
                            ),
                        },
                        None => (),
                    }
                }
            });
        events
    }

    fn update(&mut self, job_vec: Vec<JJob>, j_config: &JenkinsConfig) -> Vec<Event> {
        self.prune_missing_builds(&job_vec, j_config);
        info!("Updating with jobs: {:?}", job_vec);
        self.update_builds(job_vec, j_config)
    }

    pub fn listen(&mut self, config: Config) {
        let client = Client::new();
        loop {
            for j_config in config.job.iter() {
                match self.attempt(&client, &j_config) {
                    Ok(json) => {
                        let job_vec = json.jobs.0;
                        let mut events = self.update(job_vec, &j_config);
                        events.drain(..).for_each(|event| {
                            info!("Sending event: {:?}", event);
                            self.tx.send(event).unwrap();
                        });
                    }
                    Err(err) => {
                        error!("Request to {} failed with message {}", j_config.id, err);
                    }
                }
            }

            sleep(Duration::from_secs(config.sleep));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cache::tests::{caches, names, timestamps};
    use super::*;
    use proptest::prelude::*;
    use std::sync::mpsc::{channel, Receiver};

    prop_compose! {
        [pub] fn build_numbers()(number in any::<u32>()) -> BuildNumber {
            BuildNumber(number)
        }
    }

    prop_compose! {
        [pub] fn build_durations()(duration in any::<u32>()) -> BuildDuration {
            BuildDuration(duration)
        }
    }

    prop_compose! {
        [pub] fn build_urls()(url in any::<String>()) -> BuildUrl {
            BuildUrl(url)
        }
    }

    prop_compose! {
        [pub] fn j_builds()(result in any::<Option<String>>(),
                     timestamp in timestamps(),
                     number in build_numbers(),
                     duration in build_durations(),
                     url in build_urls(),
                     ) -> JBuild {
            JBuild { result, timestamp, number, duration, url }
        }
    }

    prop_compose! {
        [pub] fn j_jobs()(name in names(),
                   last_build in j_builds()) -> JJob {
            JJob { name, last_build }
        }
    }

    prop_compose! {
        fn j_job_vecs()(job_vec in prop::collection::vec(j_jobs(), 1..50)) -> JJobVec {
            JJobVec(job_vec)
        }
    }

    prop_compose! {
        fn j_jsons()(jobs in j_job_vecs()) -> JJson {
            JJson{ jobs }
        }
    }

    prop_compose! {
        [pub] fn j_listeners()(most_recent in caches(1, 5, 1, 10)) -> (JListener, Receiver<Event>) {
            let (tx, rx) = channel();
            (JListener { tx, most_recent }, rx)
        }
    }
}
