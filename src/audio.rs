use pipewire as pw;
use pw::{properties::properties, spa, stream::{StreamFlags, StreamRc}};
use pw::types::ObjectType;
use crate::i18n::tr;
use std::process::Command;
use std::{cell::RefCell, collections::VecDeque, rc::Rc, sync::{Arc, Mutex}};
use crate::effects::EffectChain;
use crate::effects::*;

type SharedBuf = Arc<Mutex<VecDeque<f32>>>;
type StreamPair = (StreamRc, pw::stream::StreamListener<()>);

pub enum AudioCommand {
    SelectMic(String),
    AddEffect { id: EffectId, effect: Box<dyn Effect> },
    RemoveEffect(EffectId),
    MoveEffect { id: EffectId, new_index: usize },
    SetEffectEnabled { id: EffectId, enabled: bool },
    SetParam { id: EffectId, param: &'static str, value: f32 },
    SetChainEnabled(bool),
}

pub struct Microphone {
    pub id: u32,
    pub name: String,        // node.name
    pub description: String, // node.description
}
pub enum AudioEvent {
    Connected,
    ConnectionFailed(String),
    MicAdded(Microphone),
    MicRemoved(u32),
    VirtualMicReady,
}

pub const VIRTUAL_MIC_NAME: &str = "cleanmic_virtual";
const MAX_BUFFERED: usize = 9600;

pub fn start() -> (async_channel::Receiver<AudioEvent>, pw::channel::Sender<AudioCommand>) {
    let (tx, rx) = async_channel::unbounded();
    let (cmd_tx, cmd_rx) = pw::channel::channel::<AudioCommand>();

    std::thread::spawn(move || {
        if let Err(e) = run(tx.clone(), cmd_rx) {
            let _ = tx.send_blocking(AudioEvent::ConnectionFailed(e.to_string()));
        }
    });

    (rx, cmd_tx)
}


fn run(tx: async_channel::Sender<AudioEvent>, cmd_rx: pw::channel::Receiver<AudioCommand>,) -> Result<(), pw::Error> {    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

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

            if name == VIRTUAL_MIC_NAME {
                let _ = tx_add.send_blocking(AudioEvent::VirtualMicReady);
                return;
            }

            if props.get("node.virtual") == Some("true") && name.starts_with("cleanmic") {
                return;
            }

            let _ = tx_add.send_blocking(AudioEvent::MicAdded(Microphone {id: global.id, name, description,}));
        })
        .global_remove(move |id| {
            let _ = tx_rm.send_blocking(AudioEvent::MicRemoved(id));
        }).register();


    let buf: SharedBuf = Default::default();
    let chain = Arc::new(Mutex::new(EffectChain::default()));

    let (_vmic_stream, _vmic_listener) = create_virtual_mic(&core, buf.clone()).map_err(|_| pw::Error::CreationFailed)?;

    let capture: Rc<RefCell<Option<StreamPair>>> = Rc::new(RefCell::new(None));

    let _cmd_attached = cmd_rx.attach(mainloop.loop_(), {
        let core = core.clone();
        let capture = capture.clone();
        let buf = buf.clone();
        let chain = chain.clone();
        move |cmd| match cmd {
            AudioCommand::SelectMic(name) => {
                if name == VIRTUAL_MIC_NAME || name.starts_with("cleanmic") {
                    eprintln!("Ignoriere eigene virtuelle Mic als Eingang");
                    return;
                }
                eprintln!("SelectMic: {name}");
                *capture.borrow_mut() = None;
                buf.lock().unwrap().clear();
                match create_capture(&core, &name, buf.clone(), chain.clone()) {
                    Ok(pair) => *capture.borrow_mut() = Some(pair),
                    Err(e) => eprintln!("Capture fehlgeschlagen: {e}"),
                }
            }
            AudioCommand::AddEffect { id, effect } => chain.lock().unwrap().add(id, effect),
            AudioCommand::RemoveEffect(id) => chain.lock().unwrap().remove(id),
            AudioCommand::MoveEffect { id, new_index } => chain.lock().unwrap().move_to(id, new_index),
            AudioCommand::SetEffectEnabled { id, enabled } => chain.lock().unwrap().set_slot_enabled(id, enabled),
            AudioCommand::SetParam { id, param, value } => chain.lock().unwrap().set_param(id, param, value),
            AudioCommand::SetChainEnabled(on) => chain.lock().unwrap().set_enabled(on),
        }
    });
    mainloop.run();
    Ok(())
}

pub fn set_default_source(name: &str) -> bool {
    Command::new("pactl").args(["set-default-source", name]).status() .map(|s| s.success()).unwrap_or(false)
}

fn create_virtual_mic(core: &pw::core::CoreRc, buf: SharedBuf) -> Result<StreamPair, Box<dyn std::error::Error>>{
    let stream = StreamRc::new(core.clone(), "CleanMic Virtual Mic",
        properties! {
            "node.always-process" => "true",
            "node.latency" => "1024/48000",
            *pw::keys::MEDIA_CLASS => "Audio/Source",
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::NODE_NAME => VIRTUAL_MIC_NAME,
            *pw::keys::NODE_DESCRIPTION => tr("CleanMic"),
            *pw::keys::NODE_VIRTUAL => "true",
        },)?;

    let mut primed = false;

    let listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(|_, _, old, new| {
            eprintln!("[stream] {old:?} -> {new:?}");
        })
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            let data = &mut datas[0];

            let mut n_frames = 0;
            if let Some(slice) = data.data() {
                let max = slice.len() / 4;
                n_frames = max.min(1024);

                let mut q = buf.lock().unwrap();
                if !primed && q.len() >= n_frames * 3 { primed = true; }
                if primed && q.len() < n_frames { primed = false; }

                for i in 0..n_frames {
                    let s = if primed { q.pop_front().unwrap_or(0.0) } else { 0.0 };
                    slice[i * 4..i * 4 + 4].copy_from_slice(&s.to_le_bytes());
                }
            }

            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = 4;
            *chunk.size_mut() = (n_frames * 4) as u32;
        })
        .register()?;

    let bytes = format_bytes();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).unwrap()];

    stream.connect(spa::utils::Direction::Output, None, StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS, &mut params,)?;
    Ok((stream, listener))
}

fn format_bytes() -> Vec<u8> {
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::F32LE);
    info.set_rate(48000);
    info.set_channels(1);

    let obj = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &spa::pod::Value::Object(obj),).unwrap().0.into_inner()
}

fn create_capture(core: &pw::core::CoreRc, target: &str, buf: SharedBuf, chain: Arc<Mutex<EffectChain>>,) -> Result<StreamPair, Box<dyn std::error::Error>> {
    let stream = StreamRc::new(core.clone(), "CleanMic Capture", properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        "target.object" => target,
        "node.target" => target,
        "node.dont-fallback" => "true",
        "node.dont-reconnect" => "true",
        "node.name" => "cleanmic_capture",
        "node.passive" => "false",
        },
    )?;

    let listener = stream
        .add_local_listener_with_user_data(())
        .process(move |stream, _| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            let data = &mut datas[0];

            let offset = data.chunk().offset() as usize;
            let size = data.chunk().size() as usize;
            let Some(slice) = data.data() else { return };
            let Some(bytes) = slice.get(offset..offset + size) else { return };

            let mut samples: Vec<f32> = bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();

            chain.lock().unwrap().process(&mut samples);

            let mut q = buf.lock().unwrap();
            q.extend(samples);
            while q.len() > MAX_BUFFERED {
                q.pop_front();
            }
        }).register()?;

    let bytes = format_bytes();
    let mut params = [spa::pod::Pod::from_bytes(&bytes).unwrap()];

    stream.connect(spa::utils::Direction::Input,None, StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS, &mut params, )?;
    Ok((stream, listener))
}
