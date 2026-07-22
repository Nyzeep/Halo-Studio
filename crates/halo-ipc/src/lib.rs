use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use halo_protocol::{RunSnapshot, RuntimeCommand, RuntimeEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcError {
    InvalidJson(String),
    MissingField(&'static str),
    UnknownCommand(String),
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(f, "invalid json: {message}"),
            Self::MissingField(field) => write!(f, "missing field: {field}"),
            Self::UnknownCommand(command) => write!(f, "unknown command: {command}"),
        }
    }
}

impl Error for IpcError {}

pub fn decode_command(line: &str) -> Result<RuntimeCommand, IpcError> {
    let object = parse_string_object(line)?;
    let command_type = required(&object, "type")?;

    match command_type {
        "createRun" => Ok(RuntimeCommand::create_run(
            required(&object, "runId")?,
            required(&object, "agentId")?,
            required(&object, "prompt")?,
        )),
        "getSnapshot" => Ok(RuntimeCommand::get_snapshot(required(&object, "runId")?)),
        "shutdown" => Ok(RuntimeCommand::Shutdown),
        other => Err(IpcError::UnknownCommand(other.to_string())),
    }
}

pub fn encode_event(event: &RuntimeEvent) -> String {
    format!(
        r#"{{"type":"runtimeEvent","runId":"{}","agentId":"{}","seq":{},"kind":"{}","message":"{}"}}"#,
        escape_json_string(&event.run_id),
        escape_json_string(&event.agent_id),
        event.seq,
        escape_json_string(&event.kind),
        escape_json_string(&event.message)
    )
}

pub fn encode_snapshot(snapshot: &RunSnapshot) -> String {
    let events = snapshot
        .events()
        .iter()
        .map(encode_event)
        .collect::<Vec<_>>()
        .join(",");

    format!(
        r#"{{"type":"snapshot","runId":"{}","agentId":"{}","state":"{}","lastSeq":{},"events":[{}]}}"#,
        escape_json_string(snapshot.run_id()),
        escape_json_string(snapshot.agent_id()),
        snapshot.state().as_str(),
        snapshot.last_seq(),
        events
    )
}

pub fn encode_error(message: &str) -> String {
    format!(
        r#"{{"type":"error","message":"{}"}}"#,
        escape_json_string(message)
    )
}

fn required<'a>(
    object: &'a BTreeMap<String, String>,
    field: &'static str,
) -> Result<&'a str, IpcError> {
    object
        .get(field)
        .map(String::as_str)
        .ok_or(IpcError::MissingField(field))
}

fn parse_string_object(line: &str) -> Result<BTreeMap<String, String>, IpcError> {
    let mut parser = JsonStringObjectParser::new(line);
    parser.parse_object()
}

fn escape_json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str(r#"\""#),
            '\\' => escaped.push_str(r#"\\"#),
            '\n' => escaped.push_str(r#"\n"#),
            '\r' => escaped.push_str(r#"\r"#),
            '\t' => escaped.push_str(r#"\t"#),
            '\u{08}' => escaped.push_str(r#"\b"#),
            '\u{0c}' => escaped.push_str(r#"\f"#),
            ch if ch.is_control() => {
                escaped.push_str(&format!(r#"\u{:04x}"#, ch as u32));
            }
            ch => escaped.push(ch),
        }
    }

    escaped
}

struct JsonStringObjectParser<'a> {
    input: &'a str,
    index: usize,
}

impl<'a> JsonStringObjectParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    fn parse_object(&mut self) -> Result<BTreeMap<String, String>, IpcError> {
        let mut values = BTreeMap::new();

        self.skip_whitespace();
        self.expect_char('{')?;
        self.skip_whitespace();

        if self.consume_char('}') {
            self.ensure_done()?;
            return Ok(values);
        }

        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let value = self.parse_string()?;
            values.insert(key, value);
            self.skip_whitespace();

            if self.consume_char('}') {
                break;
            }
            self.expect_char(',')?;
        }

        self.ensure_done()?;
        Ok(values)
    }

    fn parse_string(&mut self) -> Result<String, IpcError> {
        self.expect_char('"')?;
        let mut value = String::new();

        while let Some(ch) = self.next_char() {
            match ch {
                '"' => return Ok(value),
                '\\' => value.push(self.parse_escape()?),
                ch if ch.is_control() => {
                    return Err(self.error("control character in string"));
                }
                ch => value.push(ch),
            }
        }

        Err(self.error("unterminated string"))
    }

    fn parse_escape(&mut self) -> Result<char, IpcError> {
        match self.next_char() {
            Some('"') => Ok('"'),
            Some('\\') => Ok('\\'),
            Some('/') => Ok('/'),
            Some('b') => Ok('\u{08}'),
            Some('f') => Ok('\u{0c}'),
            Some('n') => Ok('\n'),
            Some('r') => Ok('\r'),
            Some('t') => Ok('\t'),
            Some('u') => self.parse_unicode_escape(),
            Some(ch) => Err(self.error(&format!("unsupported escape: {ch}"))),
            None => Err(self.error("unterminated escape")),
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, IpcError> {
        let high = self.parse_hex_quad()?;

        if (0xd800..=0xdbff).contains(&high) {
            let saved_index = self.index;
            if self.consume_char('\\') && self.consume_char('u') {
                let low = self.parse_hex_quad()?;
                if (0xdc00..=0xdfff).contains(&low) {
                    let scalar = 0x10000 + (((high - 0xd800) << 10) | (low - 0xdc00));
                    return char::from_u32(scalar)
                        .ok_or_else(|| self.error("invalid unicode surrogate pair"));
                }
            }
            self.index = saved_index;
            return Err(self.error("missing low unicode surrogate"));
        }

        char::from_u32(high).ok_or_else(|| self.error("invalid unicode scalar"))
    }

    fn parse_hex_quad(&mut self) -> Result<u32, IpcError> {
        let mut value = 0_u32;

        for _ in 0..4 {
            let ch = self
                .next_char()
                .ok_or_else(|| self.error("unterminated unicode escape"))?;
            value = (value << 4)
                | ch.to_digit(16)
                    .ok_or_else(|| self.error("invalid unicode escape"))?;
        }

        Ok(value)
    }

    fn expect_char(&mut self, expected: char) -> Result<(), IpcError> {
        match self.next_char() {
            Some(ch) if ch == expected => Ok(()),
            Some(ch) => Err(self.error(&format!("expected '{expected}', got '{ch}'"))),
            None => Err(self.error(&format!("expected '{expected}', got end of input"))),
        }
    }

    fn consume_char(&mut self, expected: char) -> bool {
        let saved_index = self.index;
        match self.next_char() {
            Some(ch) if ch == expected => true,
            _ => {
                self.index = saved_index;
                false
            }
        }
    }

    fn next_char(&mut self) -> Option<char> {
        let ch = self.input.get(self.index..)?.chars().next()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self
            .input
            .get(self.index..)
            .and_then(|rest| rest.chars().next())
        {
            if !ch.is_whitespace() {
                break;
            }
            self.index += ch.len_utf8();
        }
    }

    fn ensure_done(&mut self) -> Result<(), IpcError> {
        self.skip_whitespace();
        if self.index == self.input.len() {
            Ok(())
        } else {
            Err(self.error("trailing characters"))
        }
    }

    fn error(&self, message: &str) -> IpcError {
        IpcError::InvalidJson(format!("{message} at byte {}", self.index))
    }
}
