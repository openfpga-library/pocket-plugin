use extism_pdk::*;
use serde::{Deserialize, Serialize};
use std::{fs, time::Duration};

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
    fn print_msg(message: &str) -> ();
}

fn print(msg: &str) -> () {
    unsafe {
        let _ = print_msg(msg);
    };
}

fn println(msg: &str) -> () {
    unsafe {
        let _ = print_msg(&format!("{msg}\n"));
    };
}

#[plugin_fn]
pub fn start() -> FnResult<PluginMessage> {
    println("Plugin's running!");

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
    println("Handling response!");
    match input {
        HostMessage::Answer { name, value } => match (name.as_str(), value.as_str()) {
            ("main-menu", "Download A File") => {
                println("Going to download a file...");
                let mut progress = 0;
                while progress < 24 {
                    progress += 1;
                    print("█");
                    std::thread::sleep(Duration::from_millis(200));
                }
                print("\n");

                let request = HttpRequest::new("https://www.example.com");
                let response = http::request::<()>(&request, None);

                match response {
                    Ok(resp) => {
                        resp.body();
                        println("Did download something!");
                        fs::write("pocket/file.html", resp.body())?;
                        return Ok(PluginMessage::Exit);
                    }
                    Err(err) => {
                        error!("Failed to download something...")
                    }
                }
                Ok(PluginMessage::Exit)
            }
            ("main-menu", "Open A Website") => {
                println("Opening a website...");
                unsafe { open_url("https://example.com")? };
                Ok(PluginMessage::Exit)
            }
            ("main-menu", "Ask A Question") => {
                println("Asking a free text question...");

                Ok(PluginMessage::Text {
                    name: "sub-question".to_string(),
                    query: "How much wood would a woodchuck chuck?".to_string(),
                })
            }
            ("main-menu", "Exit") => {
                println("Exiting...");
                Ok(PluginMessage::Exit)
            }
            ("sub-question", response) => {
                println(&format!("So a woodchuck could chuck \"{response}\" wood!"));
                Ok(PluginMessage::Exit)
            }
            _ => Ok(PluginMessage::Exit),
        },
        HostMessage::Kill => Ok(PluginMessage::Exit),
    }
}
