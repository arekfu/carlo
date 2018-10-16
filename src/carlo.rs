use std::time::Instant;

#[derive(Debug)]
pub struct Carlo {
    start_time: Instant,
}

impl Carlo {
    pub fn new() -> Carlo {
        Carlo { start_time: Instant::now() }
    }

    pub fn run(&mut self) {
        use irc::client::prelude::{IrcClient, Client, ClientExt, Command};

        let client = IrcClient::new("config.toml").unwrap();
        client.identify().unwrap();

        client.for_each_incoming(|irc_msg| {
            let mut cmd_prefix = "@".to_owned();
            cmd_prefix.push_str(client.current_nickname());

            // irc_msg is a Message
            match irc_msg.command {
                Command::PRIVMSG(channel, message) => {
                    let trimmed = message.trim_left();
                    if trimmed.starts_with(&cmd_prefix) {
                        let return_msg = self.handle_msg(&message);
                        // send_privmsg comes from ClientExt
                        client.send_privmsg(&channel, return_msg).unwrap();
                    }
                },
                _ => ()
            }
        }).unwrap();
    }

    fn handle_msg(&self, message: &str) -> String {
        if message.contains("uptime") {
            format!("uptime: {} seconds",
                    self.start_time.elapsed().as_secs())
        } else {
            "beep boop".to_string()
        }
    }
}
