use pipewire as pw;
use pw::types::ObjectType;

pub struct Microphone {
    pub id: u32,
    pub name: String,        // node.name
    pub description: String, // node.description

pub enum AudioEvent {
    Connected,
    ConnectionFailed(String),
    MicAdded(Microphone),
    MicRemoved(u32),
}

pub fn start() -> async_channel::Receiver<AudioEvent> {
    let (tx, rx) = async_channel::unbounded();

    std::thread::spawn(move || {
        if let Err(e) = run(tx.clone()) {
            let _ = tx.send_blocking(AudioEvent::ConnectionFailed(e.to_string()));
        }
    });

    rx
}

fn run(tx: async_channel::Sender<AudioEvent>) -> Result<(), pw::Error> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let _ = tx.send_blocking(AudioEvent::Connected);

    let _ = tx.send_blocking(AudioEvent::Connected);

    let tx_add = tx.clone();
    let tx_rm = tx.clone();

    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            if global.type_ != ObjectType::Node {
                return;
            }
            let Some(props) = global.props else { return };


            if props.get("media.class") != Some("Audio/Source") {
                return;
            }

            let name = props.get("node.name").unwrap_or("").to_string();
            let description = props.get("node.description").unwrap_or(&name).to_string();

            let _ = tx_add.send_blocking(AudioEvent::MicAdded(Microphone {id: global.id, name, description,}));
        })
        .global_remove(move |id| {
            let _ = tx_rm.send_blocking(AudioEvent::MicRemoved(id));
        })
        .register();

    mainloop.run();
    Ok(())
}
