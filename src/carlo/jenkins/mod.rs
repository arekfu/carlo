use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::Sender;
use std::thread::sleep;
use std::time::Duration;

use reqwest::{Client, Error};

use carlo::Event;
use config::{Config, JenkinsConfig};

#[derive(Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct BuildName(pub String);

impl fmt::Display for BuildName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct BuildTimestamp(pub u64);

impl fmt::Display for BuildTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct BuildNumber(pub u32);

impl fmt::Display for BuildNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct JBuild {
    pub result: Option<String>,
    pub timestamp: BuildTimestamp,
    pub number: BuildNumber,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JJob {
    pub name: BuildName,
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
    most_recent: HashMap<(String, BuildName), BuildTimestamp>,
}

impl JListener {
    pub fn new(tx: Sender<Event>) -> JListener {
        JListener {
            tx,
            most_recent: HashMap::new(),
        }
    }

    fn attempt(&self, client: &Client, j_config: &JenkinsConfig) -> Result<JJson, Error> {
        info!(
            "Attempting connection to {} as {}",
            j_config.server, j_config.user
        );
        let mut response = client
            .get(&j_config.server)
            .basic_auth(&j_config.user, Some(&j_config.token))
            .send()?;
        response.json()
    }

    fn prune_builds_except(&mut self, build_keys: &Vec<(&str, &BuildName)>) {
        info!("Will keep {} builds", build_keys.len());
        fn in_build_keys(key: &(String, BuildName), build_keys: &Vec<(&str, &BuildName)>) -> bool {
            build_keys
                .iter()
                .any(|build_key| key.0 == build_key.0 && key.1 == *build_key.1)
        }
        self.most_recent
            .retain(|ref key, ref mut _val| in_build_keys(*key, build_keys));
        info!(
            "Builds kept after prune_missing_builds(): {}",
            self.most_recent.len()
        );
    }

    fn prune_missing_builds(&mut self, job_vec: &Vec<JJob>, j_config: &JenkinsConfig) {
        let mut build_keys = Vec::new() as Vec<(&str, &BuildName)>;
        job_vec
            .iter()
            .for_each(|ref job| build_keys.push((&j_config.server, &job.name)));
        self.prune_builds_except(&build_keys);
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
                    ()
                }
                Some(result) => {
                    let new_timestamp = job.last_build.timestamp;
                    match self
                        .most_recent
                        .insert((j_config.server.clone(), job.name.clone()), new_timestamp)
                    {
                        Some(old_timestamp) => match old_timestamp.cmp(&new_timestamp) {
                            Ordering::Less => {
                                info!("Job {} has a new build", job.name);
                                events.push(Event::UpdatedJob(
                                    j_config.server.clone(),
                                    job.name.clone(),
                                    result.clone(),
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
                        error!(
                            "Request to {}@{} failed with message {}",
                            j_config.user, j_config.server, err
                        );
                    }
                }
            }

            sleep(Duration::from_secs(config.sleep));
        }
    }
}
