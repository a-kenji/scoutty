use vte::Perform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSetting {
    NotRecognized,    // Pm=0
    Set,              // Pm=1
    Reset,            // Pm=2
    PermanentlySet,   // Pm=3
    PermanentlyReset, // Pm=4
}

#[derive(Debug, Clone)]
pub enum Event {
    Da1 {
        params: Vec<u16>,
    },
    Da2 {
        params: Vec<u16>,
    },
    Da3 {
        id: String,
    },
    XtVersion {
        version: String,
    },
    ModeReport {
        mode: u16,
        setting: ModeSetting,
    },
    KittyKeyboard {
        flags: u16,
    },
    XtSmGraphics {
        item: u16,
        status: u16,
        values: Vec<u16>,
    },
    WindowOp {
        op: u16,
        params: Vec<u16>,
    },
    OscColor {
        index: u16,
        sub_index: Option<String>,
        value: String,
    },
    KittyGraphics {
        payload: String,
    },
    Decrqss {
        valid: bool,
        payload: String,
    },
    XtGetTcap {
        name: String,
        value: Option<String>,
    },
}

pub struct ResponseParser {
    vte: vte::Parser,
    state: ParserState,
}

const MAX_PAYLOAD_LEN: usize = 64 * 1024;

struct ParserState {
    events: Vec<Event>,
    raw_bytes: Vec<u8>,
    has_da1: bool,
    // DCS accumulation
    dcs_intermediates: Vec<u8>,
    dcs_action: u8,
    dcs_params: Vec<u16>,
    dcs_payload: Vec<u8>,
    // APC pre-filter
    esc_pending: bool,
    in_apc: bool,
    apc_buf: Vec<u8>,
    apc_esc_seen: bool,
    apc_overflows: usize,
}

impl Default for ResponseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseParser {
    pub fn new() -> Self {
        Self {
            vte: vte::Parser::new(),
            state: ParserState {
                events: Vec::new(),
                raw_bytes: Vec::new(),
                has_da1: false,
                dcs_intermediates: Vec::new(),
                dcs_action: 0,
                dcs_params: Vec::new(),
                dcs_payload: Vec::new(),
                esc_pending: false,
                in_apc: false,
                apc_buf: Vec::new(),
                apc_esc_seen: false,
                apc_overflows: 0,
            },
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.state.raw_bytes.extend_from_slice(bytes);
        for &byte in bytes {
            if self.state.in_apc {
                if self.state.apc_esc_seen {
                    if byte == b'\\' {
                        // ST received, APC complete
                        let payload = String::from_utf8_lossy(&self.state.apc_buf).to_string();
                        if payload.starts_with('G') {
                            self.state.events.push(Event::KittyGraphics { payload });
                        }
                        self.state.in_apc = false;
                        self.state.apc_esc_seen = false;
                    } else {
                        self.state.apc_buf.push(0x1b);
                        self.state.apc_buf.push(byte);
                        self.state.apc_esc_seen = false;
                    }
                } else if byte == 0x1b {
                    self.state.apc_esc_seen = true;
                } else if byte == 0x9c {
                    // C1 ST
                    let payload = String::from_utf8_lossy(&self.state.apc_buf).to_string();
                    if payload.starts_with('G') {
                        self.state.events.push(Event::KittyGraphics { payload });
                    }
                    self.state.in_apc = false;
                } else if self.state.apc_buf.len() >= MAX_PAYLOAD_LEN {
                    self.state.in_apc = false;
                    self.state.apc_buf.clear();
                    self.state.apc_overflows += 1;
                } else {
                    self.state.apc_buf.push(byte);
                }
            } else if self.state.esc_pending {
                self.state.esc_pending = false;
                if byte == b'_' {
                    // ESC _ = APC start
                    self.state.in_apc = true;
                    self.state.apc_buf.clear();
                    self.state.apc_esc_seen = false;
                } else {
                    // Not APC, feed both ESC and byte to vte
                    self.vte.advance(&mut self.state, &[0x1b, byte]);
                }
            } else if byte == 0x1b {
                self.state.esc_pending = true;
            } else if byte == 0x9f {
                // C1 APC
                self.state.in_apc = true;
                self.state.apc_buf.clear();
                self.state.apc_esc_seen = false;
            } else {
                self.vte.advance(&mut self.state, &[byte]);
            }
        }
    }

    pub fn events(&self) -> &[Event] {
        &self.state.events
    }

    pub fn has_da1(&self) -> bool {
        self.state.has_da1
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.state.raw_bytes
    }

    pub fn apc_overflows(&self) -> usize {
        self.state.apc_overflows
    }
}

fn hex_decode(hex: &str) -> Option<String> {
    String::from_utf8(crate::hex::decode(hex)?).ok()
}

impl Perform for ParserState {
    fn print(&mut self, _c: char) {}
    fn execute(&mut self, _byte: u8) {}

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if ignore {
            return;
        }

        let flat: Vec<u16> = params.iter().flat_map(|sub| sub.iter().copied()).collect();

        match (intermediates, action) {
            // DA1: CSI ? Ps... c
            ([b'?'], 'c') | ([b'?', ..], 'c')
                if !intermediates.contains(&b'>') && !intermediates.contains(&b'=') =>
            {
                self.events.push(Event::Da1 { params: flat });
                self.has_da1 = true;
            }
            // DA2: CSI > Ps... c
            ([b'>'], 'c') => {
                self.events.push(Event::Da2 { params: flat });
            }
            // DECRPM: CSI ? Ps ; Pm $ y
            (intermediates, 'y') if intermediates.contains(&b'?') => {
                if flat.len() >= 2 {
                    let mode = flat[0];
                    let pm = flat[1];
                    let setting = match pm {
                        0 => ModeSetting::NotRecognized,
                        1 => ModeSetting::Set,
                        2 => ModeSetting::Reset,
                        3 => ModeSetting::PermanentlySet,
                        4 => ModeSetting::PermanentlyReset,
                        _ => ModeSetting::NotRecognized,
                    };
                    self.events.push(Event::ModeReport { mode, setting });
                }
            }
            // Kitty keyboard: CSI ? flags u
            ([b'?'], 'u') => {
                let flags = flat.first().copied().unwrap_or(0);
                self.events.push(Event::KittyKeyboard { flags });
            }
            // XTSMGRAPHICS: CSI ? Pi ; Ps ; Pv... S
            ([b'?'], 'S') => {
                if flat.len() >= 2 {
                    let item = flat[0];
                    let status = flat[1];
                    let values = flat[2..].to_vec();
                    self.events.push(Event::XtSmGraphics {
                        item,
                        status,
                        values,
                    });
                }
            }
            // Window ops: CSI Ps ; Ps ; Ps t
            ([], 't') => {
                if !flat.is_empty() {
                    let op = flat[0];
                    let rest = flat[1..].to_vec();
                    self.events.push(Event::WindowOp { op, params: rest });
                }
            }
            _ => {}
        }
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, action: char) {
        self.dcs_intermediates = intermediates.to_vec();
        self.dcs_action = action as u8;
        self.dcs_params = params.iter().flat_map(|sub| sub.iter().copied()).collect();
        self.dcs_payload.clear();
    }

    fn put(&mut self, byte: u8) {
        if self.dcs_payload.len() < MAX_PAYLOAD_LEN {
            self.dcs_payload.push(byte);
        }
    }

    fn unhook(&mut self) {
        let payload = String::from_utf8_lossy(&self.dcs_payload).to_string();

        match (self.dcs_intermediates.as_slice(), self.dcs_action) {
            // XTVERSION: DCS > | version ST
            ([b'>'], b'|') => {
                self.events.push(Event::XtVersion { version: payload });
            }
            // DECRQSS: DCS Ps $ r sgr-params m ST
            ([b'$'], b'r') => {
                let valid = self.dcs_params.first().copied().unwrap_or(0) == 1;
                self.events.push(Event::Decrqss { valid, payload });
            }
            // DA3: DCS ! | hex-id ST
            ([b'!'], b'|') => {
                self.events.push(Event::Da3 { id: payload });
            }
            // XTGETTCAP: DCS Ps +r name=value ST
            ([b'+'], b'r') => {
                let found = self.dcs_params.first().copied().unwrap_or(0) == 1;
                if let Some((name_hex, value_hex)) = payload.split_once('=') {
                    if let Some(name) = hex_decode(name_hex) {
                        let value = if found { hex_decode(value_hex) } else { None };
                        self.events.push(Event::XtGetTcap { name, value });
                    }
                } else if let Some(name) = hex_decode(&payload) {
                    self.events.push(Event::XtGetTcap { name, value: None });
                }
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.len() >= 2
            && let Ok(index_str) = std::str::from_utf8(params[0])
            && let Ok(index) = index_str.parse::<u16>()
        {
            if params.len() >= 3 {
                let sub_index = String::from_utf8_lossy(params[1]).to_string();
                let value = String::from_utf8_lossy(params[2]).to_string();
                self.events.push(Event::OscColor {
                    index,
                    sub_index: Some(sub_index),
                    value,
                });
            } else {
                let value = String::from_utf8_lossy(params[1]).to_string();
                self.events.push(Event::OscColor {
                    index,
                    sub_index: None,
                    value,
                });
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_da1() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?62;4;6c");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Da1 { params } => assert_eq!(params, &[62, 4, 6]),
            other => panic!("expected DA1, got {other:?}"),
        }
    }

    #[test]
    fn parse_da1_split_across_reads() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?63");
        parser.feed(b";4c");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Da1 { params } => assert_eq!(params, &[63, 4]),
            other => panic!("expected DA1, got {other:?}"),
        }
    }

    #[test]
    fn parse_da2() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[>1;10;0c");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Da2 { params } => assert_eq!(params, &[1, 10, 0]),
            other => panic!("expected DA2, got {other:?}"),
        }
    }

    #[test]
    fn parse_decrpm_set() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?2026;1$y");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ModeReport { mode, setting } => {
                assert_eq!(*mode, 2026);
                assert_eq!(*setting, ModeSetting::Set);
            }
            other => panic!("expected ModeReport, got {other:?}"),
        }
    }

    #[test]
    fn parse_decrpm_not_recognized() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?2027;0$y");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::ModeReport { mode, setting } => {
                assert_eq!(*mode, 2027);
                assert_eq!(*setting, ModeSetting::NotRecognized);
            }
            other => panic!("expected ModeReport, got {other:?}"),
        }
    }

    #[test]
    fn parse_xtversion() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1bP>|kitty(0.35.2)\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::XtVersion { version } => assert_eq!(version, "kitty(0.35.2)"),
            other => panic!("expected XtVersion, got {other:?}"),
        }
    }

    #[test]
    fn parse_da3() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1bP!|AABBCC\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Da3 { id } => assert_eq!(id, "AABBCC"),
            other => panic!("expected DA3, got {other:?}"),
        }
    }

    #[test]
    fn parse_osc_color() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b]11;rgb:1c1c/1c1c/1c1c\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::OscColor {
                index,
                sub_index,
                value,
            } => {
                assert_eq!(*index, 11);
                assert_eq!(*sub_index, None);
                assert_eq!(value, "rgb:1c1c/1c1c/1c1c");
            }
            other => panic!("expected OscColor, got {other:?}"),
        }
    }

    #[test]
    fn parse_osc52_clipboard() {
        let mut parser = ResponseParser::new();
        // OSC 52 ; c ; base64data ST
        parser.feed(b"\x1b]52;c;SGVsbG8=\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::OscColor {
                index,
                sub_index,
                value,
            } => {
                assert_eq!(*index, 52);
                assert_eq!(sub_index.as_deref(), Some("c"));
                assert_eq!(value, "SGVsbG8=");
            }
            other => panic!("expected OscColor, got {other:?}"),
        }
    }

    #[test]
    fn parse_osc4_palette_color() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b]4;1;rgb:cd00/0000/0000\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::OscColor {
                index,
                sub_index,
                value,
            } => {
                assert_eq!(*index, 4);
                assert_eq!(sub_index.as_deref(), Some("1"));
                assert_eq!(value, "rgb:cd00/0000/0000");
            }
            other => panic!("expected OscColor, got {other:?}"),
        }
    }

    #[test]
    fn parse_kitty_keyboard() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?1u");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::KittyKeyboard { flags } => assert_eq!(*flags, 1),
            other => panic!("expected KittyKeyboard, got {other:?}"),
        }
    }

    #[test]
    fn parse_kitty_graphics_apc() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b_Gi=1;OK\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::KittyGraphics { payload } => assert_eq!(payload, "Gi=1;OK"),
            other => panic!("expected KittyGraphics, got {other:?}"),
        }
    }

    #[test]
    fn parse_window_op() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[6;20;10t");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::WindowOp { op, params } => {
                assert_eq!(*op, 6);
                assert_eq!(params, &[20, 10]);
            }
            other => panic!("expected WindowOp, got {other:?}"),
        }
    }

    #[test]
    fn parse_decrqss() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1bP1$r48;2;150;150;150m\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::Decrqss { valid, payload } => {
                assert!(*valid);
                assert!(payload.contains("150"));
            }
            other => panic!("expected Decrqss, got {other:?}"),
        }
    }

    #[test]
    fn parse_xtsmgraphics() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?2;0;256S");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::XtSmGraphics {
                item,
                status,
                values,
            } => {
                assert_eq!(*item, 2);
                assert_eq!(*status, 0);
                assert_eq!(values, &[256]);
            }
            other => panic!("expected XtSmGraphics, got {other:?}"),
        }
    }

    #[test]
    fn parse_xtgettcap_found() {
        let mut parser = ResponseParser::new();
        // DCS 1 +r 544e=787465726d ST  → TN=xterm
        parser.feed(b"\x1bP1+r544e=787465726d\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::XtGetTcap { name, value } => {
                assert_eq!(name, "TN");
                assert_eq!(value.as_deref(), Some("xterm"));
            }
            other => panic!("expected XtGetTcap, got {other:?}"),
        }
    }

    #[test]
    fn parse_xtgettcap_not_found() {
        let mut parser = ResponseParser::new();
        // DCS 0 +r 544e ST  → TN not found (no =value)
        parser.feed(b"\x1bP0+r544e\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::XtGetTcap { name, value } => {
                assert_eq!(name, "TN");
                assert!(value.is_none());
            }
            other => panic!("expected XtGetTcap, got {other:?}"),
        }
    }

    #[test]
    fn parse_xtgettcap_not_found_with_eq() {
        let mut parser = ResponseParser::new();
        // DCS 0 +r 544e=787465726d ST → param=0 means not found even with value
        parser.feed(b"\x1bP0+r544e=787465726d\x1b\\");
        let events = parser.events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            Event::XtGetTcap { name, value } => {
                assert_eq!(name, "TN");
                assert!(value.is_none());
            }
            other => panic!("expected XtGetTcap, got {other:?}"),
        }
    }

    #[test]
    fn apc_overflow_discarded() {
        let mut parser = ResponseParser::new();
        // Start APC, feed more than 64KB without terminating
        parser.feed(b"\x1b_G");
        let big = vec![b'x'; MAX_PAYLOAD_LEN];
        parser.feed(&big);
        // The overflowed APC should be discarded, no event produced
        assert!(parser.events().is_empty());
        assert_eq!(parser.apc_overflows(), 1);
        // Parser recovers: a subsequent valid sequence still parses
        parser.feed(b"\x1b[?62;4c");
        assert_eq!(parser.events().len(), 1);
        assert!(matches!(&parser.events()[0], Event::Da1 { .. }));
    }

    #[test]
    fn apc_overflow_counter_accumulates() {
        let mut parser = ResponseParser::new();
        assert_eq!(parser.apc_overflows(), 0);

        // First overflow
        parser.feed(b"\x1b_G");
        parser.feed(&vec![b'x'; MAX_PAYLOAD_LEN]);
        assert_eq!(parser.apc_overflows(), 1);

        // Second overflow
        parser.feed(b"\x1b_G");
        parser.feed(&vec![b'y'; MAX_PAYLOAD_LEN]);
        assert_eq!(parser.apc_overflows(), 2);

        // A normal APC that fits should not increment the counter
        parser.feed(b"\x1b_Gi=1;OK\x1b\\");
        assert_eq!(parser.apc_overflows(), 2);
    }

    #[test]
    fn parse_multiple_events() {
        let mut parser = ResponseParser::new();
        parser.feed(b"\x1b[?2026;1$y\x1b[?1004;2$y\x1b[?62;4c");
        let events = parser.events();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], Event::ModeReport { mode: 2026, .. }));
        assert!(matches!(&events[1], Event::ModeReport { mode: 1004, .. }));
        assert!(matches!(&events[2], Event::Da1 { .. }));
    }
}
