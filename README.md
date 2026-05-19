# pocket-plugin
Plugins for Analogue Pocket updaters (Pocket Sync, pupdate) using extism


## To build
You'll need to `rustup target add wasm32-wasip1` if you've not got it already, then
`cargo build -p example_plugin --target wasm32-wasip1 && cargo run -p demo_host`

should run the example plugin within the demo app

## Flow

The plugin must define at least a `start` `plugin_fn` (no arguments), if it doesn't need to ask the user for input.
If it needs to ask the user for input it'll also need a `handle_response` `plugin_fn` which recieves the `HostMessage` enum, JSON serialised - which can either be a response to a `PluginMessage` that's asked for input, or a signal to kill the plugin.

The host must define a `open_url` `host_fn` which takes a url as a string and should open it in the user's browser.

The host should respond to the 3 possible `PluginMessage` options (`Choice`, `Text`, `Exit`) by:
- rendering UI with a multiple choice
- rendering UI for a free text prompt
- letting the user know the plugin's finished (or whatever makes sense, returning the user to the main app etc)

```mermaid
sequenceDiagram
    autonumber
    actor Host as Host Application
    participant Plugin as Extism Plugin
    Note over Host,Plugin: Start / Initial Handshake
    Host->>Plugin: call start()
    Note right of Plugin: Plugin runs
    Note right of Plugin: Can call open_url host_fn
    Note right of Plugin: Can call info! to write to host app log
    Note right of Plugin: Can perform file system operations & http requests etc
    Plugin-->>Host: return PluginMessage (JSON-serialized)
    Note over Plugin,Host: PluginMessage is either Choice, Text, or Exit

    Loop Active
        Note over Host: Host renders input UI
        Note right of Host: wait for input...
        Host->>Host: User provides input / selection / Kill message
        Host->>Plugin: call handle_response(HostMessage) (JSON-serialized)
        Note right of Host: HostMessage is Answer or Kill

        opt Plugin processing logic
            Note right of Plugin: Plugin handles the message

        end

        Plugin->>Host: return PluginMessage (JSON-serialized)
        Note over Plugin,Host: PluginMessage is either Choice, Text or Exit type

        break PluginMessage is Exit
            Note over Host,Plugin: Stop processing loop
        end

    end
    Note over Host: Host handles final exit state
```

## TODO
- [x] Sketch out host_fns & plugin_fns
- [x] Rough Demo & app
- [] Add another folder accessible by the plugin on the host machine (empy folder, sub-directory on the app directory, exposed as `computer/` or something)
- [] Add logic & schema for the JSON file that'll be beside a plugin that tells us the name, a description, what hosts it wants to access (with a wildcard option) etc
- [] Document how the Plugin system works for non-Rust plugins (not 100% sure how the enums are encoded etc)
- [] Generate a schema https://github.com/extism/rust-pdk#generating-bindings

## Unknowns
- We could show the plugin individual folders for Games / Saves / Cores etc, but I don't think this would give us much
