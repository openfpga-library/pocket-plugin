use clap::Parser;
use extism::convert::Json;
use extism::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListDirection, ListState, Paragraph, Wrap};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use std::{fs, vec};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use webbrowser;

// Demo app for Pocket Plugins
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the Pocket's SD card (or fake pocket folder)
    #[arg(short, long, default_value = "./fake_pocket")]
    pocket_path: PathBuf,

    /// Path to the folder on the host (for cache / safe storage)
    #[arg(long, default_value = "./host_folder")]
    host_folder: PathBuf,

    /// info, error, trace, debug, warn
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// folder to look for plugin.wasm & plugin.json in
    #[arg(short, long)]
    folder_plugin: PathBuf,
}

#[derive(Deserialize, Default, Debug)]
struct PluginInfo {
    name: String,
    description: Option<String>,
    logo_url: Option<String>,
    allowed_hosts: Vec<String>,
}

#[derive(Clone, Deserialize, FromBytes, Debug)]
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

#[derive(Clone, Serialize, Debug, ToBytes)]
#[encoding(Json)]
enum HostMessage {
    Answer { name: String, value: String },
    Kill,
}

static LOG_TX: OnceLock<mpsc::Sender<String>> = OnceLock::new();
static PLUGIN_INFO: OnceLock<PluginInfo> = OnceLock::new();
static WASM_PATH: OnceLock<PathBuf> = OnceLock::new();
static POCKET_PATH: OnceLock<PathBuf> = OnceLock::new();
static HOST_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOG_LEVEL: OnceLock<String> = OnceLock::new();

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    let pocket_path = args.pocket_path;
    if !pocket_path.exists() {
        panic!("Unable to find pocket folder {:?}", &pocket_path);
    }

    POCKET_PATH.set(pocket_path).unwrap();

    let host_path = args.host_folder;
    if !host_path.exists() {
        panic!("Unable to find host folder {:?}", &host_path);
    }

    HOST_PATH.set(host_path).unwrap();

    let wasm_path = args.folder_plugin.join("plugin.wasm");
    if !wasm_path.exists() {
        panic!("Unable to find WASM at {:?}", &wasm_path);
    }

    WASM_PATH.set(wasm_path).unwrap();

    let json_path = args.folder_plugin.join("plugin.json");
    if !json_path.exists() {
        panic!("Unable to find info JSON at {:?}", json_path);
    }

    LOG_LEVEL.set(args.log_level).unwrap();

    let file = fs::File::open(json_path)?;
    let plugin_info: PluginInfo = serde_json::from_reader(file)?;

    println!("loading plugin {}...", plugin_info.name);

    PLUGIN_INFO.set(plugin_info).unwrap();

    let (plugin_to_host_tx, plugin_to_host_rx) = mpsc::channel(16);
    let (log_tx, log_rx) = mpsc::channel(16);
    let (host_to_plugin_tx, host_to_plugin_rx) = mpsc::channel(16);

    LOG_TX.set(log_tx.clone()).unwrap();

    let mut set = JoinSet::new();
    set.spawn(async move { run_plugin(plugin_to_host_tx, host_to_plugin_rx, log_tx).await });
    set.spawn(async move { run_ui(host_to_plugin_tx, plugin_to_host_rx, log_rx).await });

    while let Some(res) = set.join_next().await {
        if let Err(e) = res {
            eprintln!("Task panicked or failed: {e}");
        }
    }

    Ok(())
}

host_fn!(pub open_url(url: &str) {
  webbrowser::open(url)?;
  Ok(())
});

host_fn!(pub print_msg(message: &str) {
  if let Some(tx) = LOG_TX.get() {
    let _ = tx.try_send(message.to_string());
  }
  Ok(())
});

async fn run_plugin(
    plugin_to_host_tx: tokio::sync::mpsc::Sender<PluginMessage>,
    mut host_to_plugin_rx: tokio::sync::mpsc::Receiver<HostMessage>,
    log_tx: tokio::sync::mpsc::Sender<String>,
) -> Result<(), anyhow::Error> {
    let log_level = LOG_LEVEL.get().unwrap();
    extism::set_log_callback(
        move |log_line| {
            let _ = log_tx.try_send(log_line.to_string());
        },
        log_level,
    )?;

    let wasm_path = WASM_PATH.get().unwrap();
    let wasm_file = Wasm::file(wasm_path);
    let plugin_info = PLUGIN_INFO.get().unwrap();
    let pocket_path = POCKET_PATH.get().unwrap();
    let host_path = HOST_PATH.get().unwrap();

    let manifest = Manifest::new([wasm_file])
        .with_allowed_path(
            pocket_path
                .to_str()
                .expect("Invalid pocket path")
                .to_string(),
            "pocket",
        )
        .with_allowed_path(
            host_path.to_str().expect("Invalid host path").to_string(),
            "host",
        )
        .with_allowed_hosts(plugin_info.allowed_hosts.clone().into_iter());

    let user_data = UserData::new(());

    let mut plugin = PluginBuilder::new(manifest)
        .with_wasi(true)
        .with_function("open_url", [PTR], [PTR], user_data.clone(), open_url)
        .with_function("print_msg", [PTR], [PTR], user_data.clone(), print_msg)
        .build()
        .unwrap();

    let res = plugin.call::<Option<()>, PluginMessage>("start", None)?;

    match res {
        PluginMessage::Choice { .. } | PluginMessage::Text { .. } => {
            let _ = plugin_to_host_tx.send(res).await;
        }
        PluginMessage::Exit => {
            return Ok(());
        }
    }

    loop {
        // Should be able to do a while let Some(message) = host_to_plugin_rx.recv().await
        // but that doesn't work
        // for some weird unknown reason
        tokio::time::sleep(Duration::from_secs(1)).await;
        if host_to_plugin_rx.len() == 0 {
            continue;
        }
        if let Ok(message) = host_to_plugin_rx.try_recv() {
            let res = plugin.call::<HostMessage, PluginMessage>("handle_response", message)?;

            match res {
                PluginMessage::Choice { .. } | PluginMessage::Text { .. } => {
                    let _ = plugin_to_host_tx.send(res).await;
                }
                PluginMessage::Exit => {
                    let _ = plugin_to_host_tx.send(res).await;
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

async fn run_ui(
    host_to_plugin_tx: tokio::sync::mpsc::Sender<HostMessage>,
    plugin_to_host_rx: tokio::sync::mpsc::Receiver<PluginMessage>,
    log_rx: tokio::sync::mpsc::Receiver<String>,
) -> Result<(), anyhow::Error> {
    let mut terminal = ratatui::init();
    let result = app(&mut terminal, host_to_plugin_tx, plugin_to_host_rx, log_rx).await;
    ratatui::restore();
    result.map_err(|e| e.into())
}

async fn app(
    terminal: &mut DefaultTerminal,
    host_to_plugin_tx: tokio::sync::mpsc::Sender<HostMessage>,
    mut plugin_to_host_rx: tokio::sync::mpsc::Receiver<PluginMessage>,
    mut log_rx: tokio::sync::mpsc::Receiver<String>,
) -> std::io::Result<()> {
    let mut logs: Vec<String> = vec![];
    let mut last_plugin_message: Option<PluginMessage> = None;

    let mut list_state = ListState::default();
    let mut text = String::from("");

    loop {
        terminal
            .draw(|frame| render(frame, &logs, &last_plugin_message, &mut list_state, &text))?;

        if crossterm::event::poll(std::time::Duration::from_millis(16))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    match key.code {
                        crossterm::event::KeyCode::Esc => {
                            // Example: Send a Kill signal to the plugin when 'ESC' is pressed
                            let _ = host_to_plugin_tx.send(HostMessage::Kill).await;
                            break Ok(());
                        }
                        crossterm::event::KeyCode::Down => {
                            list_state.select_next();
                        }

                        crossterm::event::KeyCode::Up => {
                            list_state.select_previous();
                        }
                        crossterm::event::KeyCode::Char(c) => {
                            if let Some(PluginMessage::Text { .. }) = &last_plugin_message {
                                text.push(c);
                            }
                        }
                        crossterm::event::KeyCode::Backspace => {
                            if let Some(PluginMessage::Text { .. }) = &last_plugin_message {
                                text.pop();
                            }
                        }
                        crossterm::event::KeyCode::Enter => {
                            match last_plugin_message {
                                Some(PluginMessage::Choice {
                                    name,
                                    query: _,
                                    choices,
                                }) => {
                                    if let Some(item_index) = list_state.selected() {
                                        let _ = host_to_plugin_tx
                                            .send(HostMessage::Answer {
                                                name,
                                                value: choices[item_index].to_string(),
                                            })
                                            .await;
                                        list_state.select(None);
                                    }
                                }
                                Some(PluginMessage::Text { name, query: _ }) => {
                                    let _ = host_to_plugin_tx
                                        .send(HostMessage::Answer { name, value: text })
                                        .await;
                                    text = "".to_string();
                                }
                                _ => {}
                            }
                            last_plugin_message = None;
                            text = "".to_string();
                        }

                        _ => {}
                    }
                }
            }
        }

        while let Ok(msg) = plugin_to_host_rx.try_recv() {
            last_plugin_message = Some(msg);
        }

        while let Ok(log_line) = log_rx.try_recv() {
            if logs.is_empty() {
                logs.push(String::new());
            }
            let mut parts = log_line.split('\n').peekable();

            while let Some(part) = parts.next() {
                if let Some(last_log) = logs.last_mut() {
                    last_log.push_str(part);
                }
                if parts.peek().is_some() {
                    logs.push(String::new());
                }
            }

            if logs.len() > 20 {
                logs = logs[1..].to_vec();
            }
        }
    }
}

fn render(
    frame: &mut Frame,
    logs: &[String],
    last_msg: &Option<PluginMessage>,
    list_state: &mut ListState,
    text: &str,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(frame.area());

    let logs_text = logs.join("\n");

    let plugin_info = PLUGIN_INFO.get().unwrap();

    frame.render_widget(
        Paragraph::new(logs_text)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .title(format!("{} - Plugin Logs", &plugin_info.name)),
            )
            .wrap(Wrap { trim: true }),
        layout[0],
    );

    match last_msg {
        Some(PluginMessage::Choice {
            name: _,
            query,
            choices,
        }) => {
            let list_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(25), Constraint::Fill(1)])
                .split(layout[1]);

            frame.render_widget(
                Paragraph::new("Press ESC to exit.")
                    .block(Block::new().borders(Borders::ALL).title("Controls")),
                list_layout[0],
            );

            let list = List::new(choices.iter().map(|i| i.as_str()).collect::<Vec<_>>())
                .block(Block::bordered().title(query.as_str()))
                .style(Style::new().white())
                .highlight_style(Style::new().italic())
                .highlight_symbol(">>")
                .repeat_highlight_symbol(true)
                .direction(ListDirection::TopToBottom);

            // frame.render_widget(list, list_layout[1]);
            frame.render_stateful_widget(list, list_layout[1], list_state);
        }
        Some(PluginMessage::Text { name: _, query }) => {
            let question_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints(vec![Constraint::Percentage(25), Constraint::Fill(1)])
                .split(layout[1]);

            frame.render_widget(
                Paragraph::new("Press ESC to exit.")
                    .block(Block::new().borders(Borders::ALL).title("Controls")),
                question_layout[0],
            );

            frame.render_widget(
                Paragraph::new(text)
                    .block(Block::new().borders(Borders::ALL).title(query.to_string())),
                question_layout[1],
            );

            let cursor_x = question_layout[1].x + text.chars().count() as u16 + 1;
            let cursor_y = question_layout[1].y + 1;

            frame.set_cursor_position((cursor_x, cursor_y));
        }
        Some(PluginMessage::Exit) => {
            frame.render_widget(
                Paragraph::new("Press ESC to exit. \n(Plugin has called Exit already)")
                    .block(Block::new().borders(Borders::ALL).title("Controls")),
                layout[1],
            );
        }
        _ => {
            frame.render_widget(
                Paragraph::new("Press ESC to exit.")
                    .block(Block::new().borders(Borders::ALL).title("Controls")),
                layout[1],
            );
        }
    }
}
