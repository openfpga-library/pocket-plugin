use extism_pdk::*;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Serialize, ToBytes)]
#[encoding(Json)]
enum PluginMessage {
    Choice {
        name: String,
        query: String,
        choices: Vec<String>,
    },
    Text {
        name: String,
        query: String,
    },
    Exit,
}

#[derive(Clone, Deserialize, FromBytes)]
#[encoding(Json)]
enum HostMessage {
    Answer { name: String, value: String },
    Kill,
}

#[host_fn]
unsafe extern "ExtismHost" {
    fn open_url(url: &str) -> ();
}

#[plugin_fn]
pub fn start() -> FnResult<PluginMessage> {
    info!("Plugin's running!");

    Ok(PluginMessage::Choice {
        name: "main-menu".to_string(),
        query: "Main Menu?".to_string(),
        choices: vec![
            "Download A File".to_string(),
            "Open A Website".to_string(),
            "Ask A Question".to_string(),
            "Exit".to_string(),
        ],
    })
}

#[plugin_fn]
pub fn handle_response(input: HostMessage) -> FnResult<PluginMessage> {
    info!("Handling response!");
    match input {
        HostMessage::Answer { name, value } => match (name.as_str(), value.as_str()) {
            ("main-menu", "Download A File") => {
                info!("Going to download a file...");
                let request = HttpRequest::new("https://www.example.com");
                let response = http::request::<()>(&request, None);

                match response {
                    Ok(resp) => {
                        resp.body();
                        info!("Did download something");
                        fs::write("pocket/file.html", resp.body())?;
                        return Ok(PluginMessage::Exit);
                    }
                    Err(err) => {
                        error!("Failed to download something")
                    }
                }
                Ok(PluginMessage::Exit)
            }
            ("main-menu", "Open A Website") => {
                info!("Opening a website...");
                unsafe { open_url("https://example.com")? };
                Ok(PluginMessage::Exit)
            }
            ("main-menu", "Ask A Question") => {
                info!("Asking a free text question...");

                Ok(PluginMessage::Text {
                    name: "sub-question".to_string(),
                    query: "How much wood would a woodchuck chuck?".to_string(),
                })
            }
            ("main-menu", "Exit") => {
                info!("Exiting...");
                Ok(PluginMessage::Exit)
            }
            ("sub-question", response) => {
                info!("So a woodchuck could chuck \"{response}\" wood!");
                Ok(PluginMessage::Exit)
            }
            _ => Ok(PluginMessage::Exit),
        },
        HostMessage::Kill => Ok(PluginMessage::Exit),
    }
}
