use std::collections::HashMap;
use std::fmt;

#[derive(Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct Name(String);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Timestamp(u64);

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

type ServerID = String;

#[derive(Debug, Clone)]
pub struct Cache {
    cache: HashMap<ServerID, HashMap<Name, Timestamp>>,
}

impl Cache {
    pub fn new() -> Cache {
        Cache {
            cache: HashMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        server: &ServerID,
        name: &Name,
        timestamp: &Timestamp,
    ) -> Option<Timestamp> {
        self.cache
            .entry(server.clone())
            .or_insert(HashMap::new())
            .insert(name.clone(), *timestamp)
    }

    pub fn prune_except(&mut self, server: &ServerID, build_names: &Vec<&Name>) {
        fn in_build_names(name: &Name, build_names: &Vec<&Name>) -> bool {
            build_names.iter().any(|build_name| name == *build_name)
        }
        let sub_cache = self.cache.entry(server.clone()).or_insert(HashMap::new());
        sub_cache.retain(|ref name, ref mut _val| in_build_names(*name, build_names));
        info!("Builds kept after prune_builds_except(): {}", sub_cache.len());
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use proptest::prelude::*;

    prop_compose! {
        [pub] fn server_ids()(id in any::<String>()) -> ServerID {
            id
        }
    }

    prop_compose! {
        [pub] fn names()(name in any::<String>()) -> Name {
            Name(name)
        }
    }

    prop_compose! {
        [pub] fn timestamps()(timestamp in any::<u64>()) -> Timestamp {
            Timestamp(timestamp)
        }
    }

    prop_compose! {
        [pub] fn caches(min_servers: usize, max_servers: usize, min_names: usize, max_names: usize)
            (cache in prop::collection::hash_map(
                server_ids(),
                prop::collection::hash_map(names(), timestamps(), min_servers..max_servers),
                min_names..max_names)) -> Cache {
            Cache { cache }
        }
    }

    /// Count the number of names in each subcache and return them as a new hashmap
    fn count_sizes(cache: &Cache) -> HashMap<ServerID, usize> {
        let mut counts = HashMap::new();
        cache.cache.iter().for_each(
            |(server, subcache)| {
                counts.insert(server.clone(), subcache.len());
            });
        counts
    }

    proptest! {
        #[test]
        fn keep_zero(mut cache in caches(1, 5, 1, 10)) {
            let counts = count_sizes(&cache);
            let a_server;
            {
                let server = cache.cache.keys().next().unwrap();
                a_server = server.clone();
            }
            cache.prune_except(&a_server, &Vec::new());
            cache.cache.iter().for_each(|(server, subcache)| {
                if *server == a_server {
                    assert_eq!(subcache.len(), 0);
                } else {
                    assert_eq!(subcache.len(), *counts.get(server).unwrap());
                }
            });
        }
    }

    proptest! {
        #[test]
        fn keep_one(mut cache in caches(1, 5, 1, 10)) {
            let counts = count_sizes(&cache);
            let a_server;
            let a_name;
            {
                let (server, first_sub_cache) = cache.cache.iter().next().unwrap();
                a_server = server.clone();
                let name = first_sub_cache.keys().next().unwrap();
                a_name = name.clone();
            }
            let names = vec![&a_name];
            cache.prune_except(&a_server, &names);

            cache.cache.iter().for_each(|(server, subcache)| {
                if *server == a_server {
                    assert_eq!(subcache.len(), 1);
                } else {
                    assert_eq!(subcache.len(), *counts.get(server).unwrap());
                }
            });
        }
    }
}
