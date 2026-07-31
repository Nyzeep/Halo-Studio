/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::{
    Error,
    parser::entry_parser::{Entry, EntryParser},
    pattern::Pattern,
};
use std::{
    collections::HashMap,
    // path::{Path, PathBuf},
    // env,
};

pub type HostPattern = Pattern;
pub type ConfigKey<'a> = &'a str;
pub type ConfigValue<'a> = &'a str;

pub type HostConfig<'a> = HashMap<ConfigKey<'a>, ConfigValue<'a>>;

#[derive(Debug, Clone, PartialEq)]
pub struct SSHConfig<'a> {
    entries: HashMap<HostPattern, HostConfig<'a>>,
}

impl<'a> SSHConfig<'a> {
    // /// Reads /etc/ssh/ssh_config and ~/.ssh/config and merges them
    // /// into a single config.
    // ///
    // /// Fails if reading either of those files fails
    // ///
    // /// Not available in windows, we don't know where to get those
    // #[cfg(not(target_os = "windows"))]
    // pub fn user_config() -> Result<Self, Error> {
    //     let default = SSHConfig::empty();

    //     let system_config = SSHConfig::read_file("/etc/ssh/ssh_config")?;

    //     let home = PathBuf::from(env::var("HOME")?);
    //     let user_config = SSHConfig::read_file(home.join("/.ssh/config"))?;

    //     let config = default
    //         .merge(system_config)
    //         .merge(user_config);

    //     Ok(config)
    // }

    /// Creates an empty config
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new()
        }
    }


    /// Query a host for its settings
    pub fn query<Q: AsRef<str>>(&self, query: Q) -> HostConfig<'a> {
        let query = query.as_ref();

        self.entries
            .keys()
            .fold(HashMap::new(), |mut acc, host| if host.matches(query) {
                let host_settings = &self.entries[host];
                acc.extend(host_settings);
                acc
            } else {
                acc
            })
    }

    // /// Parses a config from a file path
    // pub fn read_file<P: AsRef<Path>>(_path: P) -> Result<Self, Error> {
    //     // let contents = fs::read_to_string(filename)?;
    //     // SSHConfig::read_str(&contents[..])
    //     unimplemented!()
    // }

    /// Parses a config from a source str
    pub fn parse_str(source: &'a str) -> Result<Self, Error> {
        // Fails the entire operation on the first parser error
        let all_entries = EntryParser::new(source)
            .collect::<Result<Vec<Entry>, Error>>()?;

        // TODO: It would be cool if we could do this all in a single iterator
        let entries = all_entries.into_iter()
            .fold(HashMap::new(), |mut hm, entry| {
                let host = HostPattern::new(entry.host);
                if !hm.contains_key(&host) {
                    hm.insert(host.clone(), HashMap::new());
                }

                let host_hm = hm.get_mut(&host)
                    .unwrap(); // We are safe to unwrap because we just created the HashMap above

                host_hm.insert(entry.key, entry.value);

                hm
            });

        Ok(Self { entries })
    }


    // /// Merges two configs.
    // ///
    // /// Entries from `other` have priority over merges from self
    // pub fn merge(self, _other: Self) -> Self {
    //     unimplemented!()
    // }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_simple() {
        let config = SSHConfig::parse_str(r#"
        Host test-host
          Port 22
          Username user
        "#).unwrap();

        let host_settings = config.query("test-host");
        assert_eq!(host_settings["Port"], "22");
        assert_eq!(host_settings["Username"], "user");
    }

    #[test]
    fn config_multi() {
        let config = SSHConfig::parse_str(r#"
        Host test-host,other-host
          Port 22
          Username user
        "#).unwrap();

        let host_settings = config.query("test-host");
        assert_eq!(host_settings["Port"], "22");
        assert_eq!(host_settings["Username"], "user");

        let other_host_settings = config.query("test-host");
        assert_eq!(other_host_settings["Port"], "22");
        assert_eq!(other_host_settings["Username"], "user");
    }
}
