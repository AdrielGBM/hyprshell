[logic]
use crate::form::{
    ALIGNS, EDGES, align_str, edge_str, parse_align, parse_edge, parse_u64, persist, source,
};
use ::config::OsdConfig;

let (config, path) = source();
let o = &config.osd;
let base = *o;
let edge = signal(edge_str(o.edge).to_string());
let align = signal(align_str(o.align).to_string());
let timeout = signal(o.timeout_ms.to_string());

let save: Box<dyn Fn()> = Box::new({
    let (edge, align, timeout) = (edge.clone(), align.clone(), timeout.clone());
    move || {
        let value = OsdConfig {
            edge: parse_edge(&edge.peek()),
            align: parse_align(&align.peek()),
            timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
        };
        persist(&path, "osd", &value);
    }
});

[view]
form_section title(|| telar::t!("settings.section.osd"))
    enum_row label(|| telar::t!("settings.field.edge")) value:$edge options:EDGES
    enum_row label(|| telar::t!("settings.field.align")) value:$align options:ALIGNS
    text_row label(|| telar::t!("settings.field.timeout_ms")) value:$timeout placeholder:"1200"
    save_row label(|| telar::t!("settings.save.osd")) on_press(save)
