use std::time::Instant;

use std::sync::mpsc;

use std::sync::Arc;

use std::thread;

use irc::client::prelude::{Client, ClientExt, Command, IrcClient};
use irc::proto::message::Message;
use irc::proto::ChannelExt;

use config::Config;
use irc_listener::IrcListener;
use j_listener::{BuildName, JListener};

#[derive(Debug)]
pub struct Carlo {
    start_time: Instant,
    client: Arc<IrcClient>,
    jenkins_config: Option<Config>,
}

#[derive(Debug)]
pub enum Event {
    IncomingIrcMessage(Message),
    UpdatedJob(String, BuildName, String, Vec<String>),
}

impl Carlo {
    pub fn new() -> Carlo {
        debug!("New Carlo instance");
        Carlo {
            start_time: Instant::now(),
            client: Arc::new(IrcClient::new("irc.toml").expect("Could not find irc.toml file")),
            jenkins_config: Config::from_file("jenkins.toml")
                .map_err(|err| warn!("Config could not be read: {}", err))
                .ok(),
        }
    }

    pub fn run(&mut self) {
        let (tx, rx) = mpsc::channel();

        debug!("Identifying with server");
        self.client.identify().unwrap();

        let mut handles = Vec::new();

        let irclistener = IrcListener::new(self.client.clone(), tx.clone());

        handles.push(thread::spawn(move || irclistener.listen()));

        if let Some(config) = self.jenkins_config.take() {
            let mut jlistener = JListener::new(tx.clone());
            handles.push(thread::spawn(move || jlistener.listen(config)));
        }

        rx.iter().for_each(|event| {
            let mut messages = self.handle(event);
            messages.drain(..).for_each(|message| {
                info!("Sending {}", message);
                self.client.send(message).unwrap();
            });
        });
        handles.drain(..).for_each(|handle| handle.join().unwrap());
    }

    fn handle(&self, event: Event) -> Vec<Message> {
        debug!("Handling event {:?}", event);
        match event {
            Event::IncomingIrcMessage(message) => self.handle_irc(message),
            Event::UpdatedJob(server, name, result, notify) => {
                self.handle_updated_job(server, name, result, notify)
            }
        }
    }

    fn handle_irc(&self, message: Message) -> Vec<Message> {
        debug!("Handling Irc message {:?}", message);
        let cmd_prefix = self.client.current_nickname().to_string();
        match &message.command {
            Command::PRIVMSG(channel, msg) => {
                if !channel.is_channel_name() || msg.trim_left().starts_with(&cmd_prefix) {
                    let reply_to = message.response_target().unwrap().to_string();
                    let source_nick = message.source_nickname().unwrap_or("");
                    self.process_msg(&source_nick, &reply_to, &msg)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn handle_updated_job(
        &self,
        server: String,
        name: BuildName,
        result: String,
        mut notify: Vec<String>,
    ) -> Vec<Message> {
        debug!(
            "Handling Job update {:?}:{:?}:{:?}:{:?}",
            server, name, result, notify
        );
        notify
            .drain(..)
            .map(|dest| {
                let reply = format!(
                    "New build for job '{}' on '{}'! Result: {}",
                    name, server, result
                );
                let cmd = Command::PRIVMSG(dest, reply);
                Message::from(cmd)
            }).collect()
    }

    fn process_msg(&self, source_nick: &str, reply_to: &str, incoming: &str) -> Vec<Message> {
        if incoming.contains("uptime") {
            info!(
                "\"uptime\" command received from {} on {}",
                source_nick, reply_to
            );
            let reply = format!("uptime = {} seconds", self.start_time.elapsed().as_secs());
            let cmd = Command::PRIVMSG(reply_to.to_string(), reply);
            return vec![Message::from(cmd)];
        } else if incoming.starts_with("say ") {
            info!(
                "\"say\" command received from {} on {}",
                source_nick, reply_to
            );
            if !self.client.config().is_owner(source_nick) {
                return Vec::new();
            }
            let v: Vec<&str> = incoming[4..].trim().splitn(2, ' ').collect();
            if v.len() <= 1 {
                debug!("\"say\" command has no message, not doing anything");
                return Vec::new();
            } else {
                let chan = v[0].to_string();
                let reply = v[1].trim().to_string();
                let cmd = Command::PRIVMSG(chan, reply);
                return vec![Message::from(cmd)];
            }
        } else {
            debug!("unrecognized command: {}", incoming);
        }
        Vec::new()
    }
}
