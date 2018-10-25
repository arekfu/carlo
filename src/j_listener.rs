use std::sync::mpsc::Sender;
use std::thread::sleep;
use std::time::Duration;
use std::collections::HashMap;
use std::fmt;

use reqwest::{Client, Error};

use carlo::Event;
use config::{Config, JenkinsConfig};

#[derive(Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct BuildName(String);

impl fmt::Display for BuildName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}


#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct BuildTimestamp(u64);

impl fmt::Display for BuildTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct BuildNumber(u32);

impl fmt::Display for BuildNumber {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct JBuild {
    result: String,
    timestamp: BuildTimestamp,
    number: BuildNumber,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JJob {
    name: BuildName,
    last_build: JBuild,
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
    most_recent: HashMap<BuildName, BuildTimestamp>
}

impl JListener {
    pub fn new(tx: Sender<Event>) -> JListener {
        JListener {
            tx,
            most_recent: HashMap::new(),
        }
    }

    fn attempt(&self, client: &Client, j_config: &JenkinsConfig) -> Result<JJson, Error>
    {
        info!("Attempting connection to {} as {}", j_config.server, j_config.user);
        let mut response = client.get(&j_config.server)
            .basic_auth(&j_config.user, Some(&j_config.token))
            .send()?;
        response.json()
    }

    fn remove_builds_except(&mut self, build_names: &Vec<&BuildName>) {
        self.most_recent.retain(|ref name, ref mut _val| build_names.iter().any(|key| key == name));
    }

    fn remove_missing_builds(&mut self, job_vec: &Vec<JJob>) {
        let mut build_names = Vec::new() as Vec<&BuildName>;
        job_vec.iter().for_each(|ref job| build_names.push(&job.name));
        self.remove_builds_except(&build_names);
    }

    fn update_builds(&mut self, mut job_vec: Vec<JJob>) -> Vec<Event> {
        let mut events = Vec::new();
        job_vec
            .drain(..)
            .for_each(|job| {
                let new_timestamp = job.last_build.timestamp;
                match self.most_recent.insert(job.name.clone(), new_timestamp) {
                    Some(old_timestamp) => {
                        if old_timestamp < new_timestamp {
                            events.push(Event::UpdatedJob(job));
                        } else {
                            warn!("Job {} went back in time from timestamp {} to {}",
                                  job.name, old_timestamp, new_timestamp);
                        }
                    },
                    None => ()
                }
            });
        debug!("{:?}", self.most_recent);
        job_vec.iter().for_each(|job| {
            match self.most_recent.get(&job.name) {
                Some(timestamp) => {
                    if *timestamp < job.last_build.timestamp {
                        events.push(Event::UpdatedJob(job.clone()));
                    }
                },
                None => ()
            }
        });
        events
    }

    fn update(&mut self, job_vec: Vec<JJob>) -> Vec<Event> {
        self.remove_missing_builds(&job_vec);
        self.update_builds(job_vec)
    }

    pub fn listen(&mut self, config: Config) {
        let client = Client::new();
        let mut n_failures = 0 as u32;
        loop {
            for j_config in config.job.iter() {
                match self.attempt(&client, &j_config) {
                    Ok(json) => {
                        n_failures = 0;
                        let job_vec = json.jobs.0;
                        let mut events = self.update(job_vec);
                        events.drain(..).for_each(|event| {
                            self.tx.send(event).unwrap();
                        });
                    },
                    Err(err) => {
                        n_failures += 1;
                        error!("Request to {}@{} failed with message {}; {} failed attempts failed so far",
                               j_config.user, j_config.server, err, n_failures);
                    }
                }
            }

            sleep(Duration::from_secs(config.sleep));
        }
    }

}
