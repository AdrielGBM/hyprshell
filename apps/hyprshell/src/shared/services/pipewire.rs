//! The audio graph as one shared source, read from PipeWire itself.
//!
//! This replaces a `wpctl` fork every two seconds. Two things were wrong with that beyond the cost: a poll
//! answers one question per fork, so a level the user changed from another mixer took up to two seconds to
//! reach the bar; and `wpctl get-volume` returns a single number, which is why the shell could show a level
//! and a mute state and nothing else — no device list, no per-application stream.
//!
//! `pw-dump --monitor` is the whole graph, once, and then a line per change. One subprocess for the shell,
//! parsed on one thread, published as one snapshot. `wpctl` stays for *mutations*: it already resolves
//! `@DEFAULT_AUDIO_SINK@` and does the volume curve, and writing was never the part that needed fixing.
//!
//! A machine with no PipeWire publishes nothing at all, which is the same thing the poll did when `wpctl` was
//! missing — every consumer already treats `None` as "no audio here".

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use platform_layershell::EventSender;
use serde_json::Value;

use crate::shared::services::broadcast::{Broadcast, Service};

/// How long to wait before re-attaching after the monitor exits. Only ever reached when PipeWire itself
/// restarted, which is when the shell most needs to come back rather than stay blank.
const REATTACH: Duration = Duration::from_secs(3);

/// What a node is to a mixer. Everything else in the graph — ports, links, the MIDI bridge, the dummy driver —
/// is not something a user adjusts, so it never reaches a snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// An output device: speakers, headphones, an HDMI port.
    Sink,
    /// An input device: a microphone.
    Source,
    /// An application playing audio.
    OutputStream,
    /// An application recording audio.
    InputStream,
}

impl NodeKind {
    fn parse(media_class: &str) -> Option<Self> {
        match media_class {
            "Audio/Sink" => Some(Self::Sink),
            "Audio/Source" | "Audio/Source/Virtual" => Some(Self::Source),
            "Stream/Output/Audio" => Some(Self::OutputStream),
            "Stream/Input/Audio" => Some(Self::InputStream),
            _ => None,
        }
    }

    pub fn is_stream(self) -> bool {
        matches!(self, Self::OutputStream | Self::InputStream)
    }
}

/// One adjustable thing in the graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// PipeWire's object id — what a mutation targets, and stable for the node's lifetime.
    pub id: u32,
    /// `node.name`: the stable identifier the default-device metadata refers to.
    pub name: String,
    /// `node.description`, else `node.nick`, else the name — what a device row shows.
    pub description: String,
    /// `application.name` for a stream; empty for a device.
    pub app: String,
    /// `media.name` for a stream — usually the track or the tab title.
    pub media: String,
    /// The icon the application asked to be drawn with, if any.
    pub icon: String,
    pub kind: NodeKind,
    /// 0–100+, on the same curve `wpctl` and every graphical mixer show.
    pub level: i32,
    pub muted: bool,
}

impl Node {
    /// What to call this node on screen: the application's name for a stream, the device description for a
    /// device, and never an empty string — a row with no label is a row a user cannot choose between.
    pub fn label(&self) -> String {
        for candidate in [&self.app, &self.description, &self.name] {
            let candidate = candidate.trim();
            if !candidate.is_empty() {
                return candidate.to_string();
            }
        }
        format!("#{}", self.id)
    }
}

/// One reading of the graph: everything adjustable, plus which device is the default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    /// `node.name` of the default sink, from PipeWire's `default` metadata.
    pub default_sink: String,
    pub default_source: String,
}

impl Graph {
    pub fn of_kind(&self, kind: NodeKind) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(move |node| node.kind == kind)
    }

    pub fn sinks(&self) -> impl Iterator<Item = &Node> {
        self.of_kind(NodeKind::Sink)
    }

    pub fn sources(&self) -> impl Iterator<Item = &Node> {
        self.of_kind(NodeKind::Source)
    }

    /// The applications currently playing, loudest-lived first in graph order.
    pub fn playback_streams(&self) -> impl Iterator<Item = &Node> {
        self.of_kind(NodeKind::OutputStream)
    }

    pub fn default_sink(&self) -> Option<&Node> {
        self.sinks().find(|node| node.name == self.default_sink)
    }

    pub fn default_source(&self) -> Option<&Node> {
        self.sources().find(|node| node.name == self.default_source)
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }
}

/// A reader of the audio graph.
type GraphHandler = Box<dyn FnMut(&Graph) + Send>;

static HANDLERS: Mutex<Vec<GraphHandler>> = Mutex::new(Vec::new());
static MONITOR_THREAD: OnceLock<()> = OnceLock::new();

/// Registers `handler` on the audio graph, attaching to PipeWire on first use.
///
/// The same shape the Hyprland event stream uses, and for the same reason: `pw-dump --monitor` is one
/// subprocess and one parse, and every derived reading — the default sink's level, the microphone, a device
/// list, a per-application stream — comes off the same batches. A monitor per consumer would be a subprocess
/// per consumer on top of the shared-source design that already rules out one per bar.
pub fn on_graph(handler: GraphHandler) {
    HANDLERS.lock().unwrap().push(handler);
    MONITOR_THREAD.get_or_init(|| {
        let _ = std::thread::Builder::new()
            .name("hyprshell-pipewire".to_string())
            .spawn(run);
    });
}

static GRAPH: Service<Graph> = Service::new("hyprshell-pipewire-graph", run_graph);

/// Mirrors the shared stream into a broadcast, so a surface can `watch` the graph like any other service. The
/// producer registers and returns; the handler owns the `Arc`, so no thread parks here.
fn run_graph(service: &Arc<Broadcast<Graph>>) {
    let published = Arc::clone(service);
    on_graph(Box::new(move |graph| published.publish(graph.clone())));
}

pub fn subscribe(tx: EventSender<Graph>) {
    GRAPH.subscribe(tx);
}

/// The last published graph, without touching PipeWire — what a click handler acts on.
pub fn current() -> Option<Graph> {
    GRAPH.current()
}

/// Publishes a graph the shell itself just caused, so a chip moves on the same frame instead of waiting for
/// the monitor to report back. The reading that follows reconciles whatever PipeWire actually accepted.
pub fn publish(graph: Graph) {
    GRAPH.publish(graph);
}

fn run() {
    let mut attached = false;
    loop {
        match monitor() {
            Ok(()) => {
                attached = true;
                tracing::warn!("pw-dump exited; re-attaching to the audio graph");
            }
            // Nothing to attach to and nothing that will change that: a machine without PipeWire is not one
            // that grows it while the shell runs, so retrying forever would just fork a doomed process a
            // thousand times a day. Retiring here is what a shell with no audio at all already does.
            Err(e) if !attached => {
                tracing::info!("no PipeWire audio graph ({e}); the audio modules will stay empty");
                return;
            }
            Err(e) => tracing::warn!("cannot re-attach to PipeWire ({e}); retrying"),
        }
        std::thread::sleep(REATTACH);
    }
}

/// Runs one `pw-dump --monitor` to completion, publishing a snapshot per batch that changed something.
///
/// `--raw` is what makes this streamable: it prints one JSON array per line — the whole graph first, then a
/// batch per change — so a line is a complete message and no bracket counting is needed.
fn monitor() -> std::io::Result<()> {
    let mut child = Command::new("pw-dump")
        .args(["--monitor", "--raw", "--no-colors"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("pw-dump gave no stdout"))?;

    let mut mirror = Mirror::default();
    let mut published = Graph::default();
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        if !mirror.apply(&line) {
            continue;
        }
        let graph = mirror.snapshot();
        if graph != published {
            published = graph.clone();
            for handler in HANDLERS.lock().unwrap().iter_mut() {
                handler(&graph);
            }
        }
    }
    let _ = child.wait();
    Ok(())
}

/// The graph as the shell currently believes it to be.
///
/// Kept as parsed nodes rather than raw JSON: a batch carries each changed object's *complete* `info`, so an
/// update is a replacement and there is nothing to merge — but it also carries objects of every type, and
/// holding onto the ports, links and clients would be most of the memory for none of the answers.
#[derive(Default)]
struct Mirror {
    nodes: HashMap<u32, Node>,
    default_sink: String,
    default_source: String,
}

impl Mirror {
    /// Applies one batch. Returns whether anything the shell draws could have changed, so a graph full of link
    /// and port churn — which is most of what a busy machine emits — costs a parse and nothing more.
    fn apply(&mut self, line: &str) -> bool {
        let Ok(Value::Array(batch)) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let mut touched = false;
        for object in batch {
            let Some(id) = object.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let id = id as u32;
            // A removed object arrives as its id with a null `info` — an application closing, a device
            // unplugged. Nothing else distinguishes it from an update.
            if object.get("info").is_some_and(Value::is_null) {
                touched |= self.nodes.remove(&id).is_some();
                continue;
            }
            match object.get("type").and_then(Value::as_str) {
                Some("PipeWire:Interface:Node") => touched |= self.apply_node(id, &object),
                Some("PipeWire:Interface:Metadata") => touched |= self.apply_metadata(&object),
                _ => {}
            }
        }
        touched
    }

    fn apply_node(&mut self, id: u32, object: &Value) -> bool {
        match parse_node(id, object) {
            Some(node) => {
                let changed = self.nodes.get(&id) != Some(&node);
                self.nodes.insert(id, node);
                changed
            }
            // A node the shell does not adjust, or one whose params have not arrived yet. Dropping a
            // previously-known node here matters: a stream keeps its id while its media class is rewritten.
            None => self.nodes.remove(&id).is_some(),
        }
    }

    /// PipeWire keeps the default device in a metadata object rather than on the node, because "default" is a
    /// property of the session and not of the hardware. It names the node, not its id.
    fn apply_metadata(&mut self, object: &Value) -> bool {
        if object
            .get("props")
            .and_then(|props| props.get("metadata.name"))
            .and_then(Value::as_str)
            != Some("default")
        {
            return false;
        }
        let Some(entries) = object.get("metadata").and_then(Value::as_array) else {
            return false;
        };
        let mut changed = false;
        for entry in entries {
            let key = entry.get("key").and_then(Value::as_str).unwrap_or_default();
            let name = entry
                .get("value")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let slot = match key {
                "default.audio.sink" => &mut self.default_sink,
                "default.audio.source" => &mut self.default_source,
                _ => continue,
            };
            if *slot != name {
                *slot = name;
                changed = true;
            }
        }
        changed
    }

    fn snapshot(&self) -> Graph {
        let mut nodes: Vec<Node> = self.nodes.values().cloned().collect();
        // Stable order, so a redraw never reshuffles a device list under the pointer. Ids ascend with
        // creation, which puts the machine's own devices above the applications that came later.
        nodes.sort_by_key(|node| node.id);
        Graph {
            nodes,
            default_sink: self.default_sink.clone(),
            default_source: self.default_source.clone(),
        }
    }
}

fn parse_node(id: u32, object: &Value) -> Option<Node> {
    let info = object.get("info")?;
    let props = info.get("props")?;
    let text = |key: &str| {
        props
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let kind = NodeKind::parse(props.get("media.class").and_then(Value::as_str)?)?;
    let volume = info
        .get("params")
        .and_then(|params| params.get("Props"))
        .and_then(Value::as_array)
        .and_then(|entries| entries.first());

    let description = [
        text("node.description"),
        text("node.nick"),
        text("node.name"),
    ]
    .into_iter()
    .find(|candidate| !candidate.trim().is_empty())
    .unwrap_or_default();

    Some(Node {
        id,
        name: text("node.name"),
        description,
        app: text("application.name"),
        media: text("media.name"),
        icon: text("application.icon-name"),
        kind,
        level: volume.map(level_of).unwrap_or(0),
        muted: volume
            .and_then(|v| v.get("mute"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The node's level on the curve a user has seen everywhere else.
///
/// PipeWire stores a linear amplitude, and every mixer on the machine — `wpctl`, pavucontrol, anything built on
/// PulseAudio's API — shows its cube root: 40% on screen is 0.064 in the graph. Reading `channelVolumes` as a
/// percentage would have the shell report 6% for a sink every other tool calls 40%.
///
/// The loudest channel rather than the first: a node with one channel turned down is not at that channel's
/// level, and a balance a user set elsewhere should not read as the volume having dropped.
fn level_of(props: &Value) -> i32 {
    let loudest = props
        .get("channelVolumes")
        .and_then(Value::as_array)
        .map(|channels| {
            channels
                .iter()
                .filter_map(Value::as_f64)
                .fold(0.0, f64::max)
        })
        .or_else(|| props.get("volume").and_then(Value::as_f64))
        .unwrap_or(0.0);
    (loudest.max(0.0).cbrt() * 100.0).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(json: &str) -> String {
        json.to_string()
    }

    const SINK: &str = r#"[{"id":54,"type":"PipeWire:Interface:Node","info":{
        "props":{"media.class":"Audio/Sink","node.name":"alsa_output.analog-stereo",
                 "node.description":"Built-in Audio"},
        "params":{"Props":[{"channelVolumes":[0.064012,0.064012],"mute":false}]}}}]"#;

    #[test]
    fn a_level_reads_on_the_curve_every_other_mixer_shows() {
        // The regression this exists for: 0.064 is what PipeWire stores for the 40% `wpctl` reports.
        let props: Value =
            serde_json::from_str(r#"{"channelVolumes":[0.064012,0.064012]}"#).unwrap();
        assert_eq!(level_of(&props), 40);
        let silent: Value = serde_json::from_str(r#"{"channelVolumes":[0.0]}"#).unwrap();
        assert_eq!(level_of(&silent), 0);
        let full: Value = serde_json::from_str(r#"{"channelVolumes":[1.0]}"#).unwrap();
        assert_eq!(level_of(&full), 100);
        // A boosted sink reads past 100 rather than clamping here; callers clamp for display.
        let boosted: Value = serde_json::from_str(r#"{"channelVolumes":[2.744]}"#).unwrap();
        assert_eq!(level_of(&boosted), 140);
    }

    #[test]
    fn the_loudest_channel_is_the_level() {
        let unbalanced: Value = serde_json::from_str(r#"{"channelVolumes":[0.064,0.0]}"#).unwrap();
        assert_eq!(
            level_of(&unbalanced),
            40,
            "a channel turned down is a balance, not a quieter sink"
        );
    }

    #[test]
    fn a_node_parses_into_something_adjustable() {
        let mut mirror = Mirror::default();
        assert!(mirror.apply(&batch(SINK)), "a new sink is a change");
        let graph = mirror.snapshot();
        let sink = graph.sinks().next().expect("the sink is listed");
        assert_eq!(sink.id, 54);
        assert_eq!(sink.level, 40);
        assert!(!sink.muted);
        assert_eq!(sink.label(), "Built-in Audio");
        assert_eq!(
            mirror.apply(&batch(SINK)),
            false,
            "the same reading twice is not a change, so nothing republishes"
        );
    }

    #[test]
    fn a_null_info_removes_the_node_it_names() {
        let mut mirror = Mirror::default();
        mirror.apply(&batch(SINK));
        assert!(
            mirror.apply(r#"[{"id":54,"info":null}]"#),
            "removal is a change"
        );
        assert!(mirror.snapshot().nodes.is_empty());
        assert!(
            !mirror.apply(r#"[{"id":54,"info":null}]"#),
            "removing what is already gone changes nothing"
        );
    }

    #[test]
    fn the_default_device_comes_from_the_metadata_not_the_node() {
        let mut mirror = Mirror::default();
        mirror.apply(&batch(SINK));
        assert!(mirror.apply(
            r#"[{"id":36,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
                 "metadata":[{"key":"default.audio.sink","value":{"name":"alsa_output.analog-stereo"}}]}]"#
        ));
        let graph = mirror.snapshot();
        assert_eq!(
            graph.default_sink().map(|n| n.id),
            Some(54),
            "the metadata names the node, and the graph resolves it"
        );
        // Another session's metadata must not be read as this one's defaults.
        assert!(!mirror.apply(
            r#"[{"id":37,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"route-settings"},
                 "metadata":[{"key":"default.audio.sink","value":{"name":"nonsense"}}]}]"#
        ));
        assert_eq!(mirror.snapshot().default_sink, "alsa_output.analog-stereo");
    }

    #[test]
    fn everything_that_is_not_a_mixer_control_is_dropped() {
        let mut mirror = Mirror::default();
        assert!(
            !mirror.apply(
                r#"[{"id":48,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Midi/Bridge","node.name":"Midi-Bridge"}}},
                    {"id":9,"type":"PipeWire:Interface:Port","info":{"props":{}}},
                    {"id":31,"type":"PipeWire:Interface:Node","info":{"props":{"node.name":"Dummy-Driver"}}}]"#
            ),
            "ports, drivers and the MIDI bridge are not things a user adjusts"
        );
        assert!(mirror.snapshot().nodes.is_empty());
    }

    #[test]
    fn a_stream_is_labelled_by_its_application() {
        let mut mirror = Mirror::default();
        mirror.apply(
            r#"[{"id":73,"type":"PipeWire:Interface:Node","info":{
                "props":{"media.class":"Stream/Output/Audio","node.name":"Firefox",
                         "application.name":"Firefox","media.name":"A video"},
                "params":{"Props":[{"channelVolumes":[1.0],"mute":true}]}}}]"#,
        );
        let graph = mirror.snapshot();
        let stream = graph
            .playback_streams()
            .next()
            .expect("the stream is listed");
        assert_eq!(stream.label(), "Firefox");
        assert_eq!(stream.media, "A video");
        assert!(stream.muted);
        assert!(stream.kind.is_stream());
    }

    /// Parses this machine's actual graph. Gated because it shells out — the suite must stay hermetic — but it
    /// is the only check that the shapes above still match what `pw-dump` emits, which is a contract PipeWire
    /// owns and can change under us.
    ///
    /// `HYPRSHELL_PIPEWIRE_LIVE=1 cargo test -p hyprshell --lib live_graph -- --nocapture`
    #[test]
    fn live_graph_parses_on_this_machine() {
        if std::env::var("HYPRSHELL_PIPEWIRE_LIVE").is_err() {
            eprintln!("set HYPRSHELL_PIPEWIRE_LIVE to parse the real graph; skipping");
            return;
        }
        let out = Command::new("pw-dump")
            .args(["--raw", "--no-colors"])
            .output()
            .expect("pw-dump runs");
        let dump = String::from_utf8_lossy(&out.stdout);
        let mut mirror = Mirror::default();
        assert!(mirror.apply(dump.trim()), "the initial dump is a change");

        let graph = mirror.snapshot();
        for node in &graph.nodes {
            eprintln!(
                "{:>4}  {:<14} {:>4}%{}  {}",
                node.id,
                format!("{:?}", node.kind),
                node.level,
                if node.muted { " muted" } else { "      " },
                node.label()
            );
        }
        eprintln!(
            "default sink: {:?}  source: {:?}",
            graph.default_sink().map(Node::label),
            graph.default_source().map(Node::label)
        );
        assert!(
            graph.sinks().next().is_some(),
            "a machine running PipeWire has at least one sink"
        );
        assert!(
            graph.default_sink().is_some(),
            "the default metadata resolves to a node that is in the graph"
        );
    }

    #[test]
    fn a_malformed_line_is_ignored_rather_than_ending_the_monitor() {
        let mut mirror = Mirror::default();
        mirror.apply(&batch(SINK));
        assert!(!mirror.apply("not json at all"));
        assert!(!mirror.apply("{}"), "a bare object is not a batch");
        assert_eq!(mirror.snapshot().nodes.len(), 1, "the graph survives it");
    }
}
