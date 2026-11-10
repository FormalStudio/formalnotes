//! Server-sent events, line by line: `event:` and `data:` fields up to a blank line.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    /// The `event:` field; empty when the stream names none (chat completions).
    pub name: String,
    pub data: String,
}

#[derive(Default)]
pub struct Lines {
    current: Event,
    has_data: bool,
}

impl Lines {
    /// Takes one line (without its newline); returns the event a blank line completes.
    pub fn push(&mut self, line: &str) -> Option<Event> {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            return self.finish();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => self.current.name = value.to_owned(),
            "data" => {
                if self.has_data {
                    self.current.data.push('\n');
                }
                self.current.data.push_str(value);
                self.has_data = true;
            }
            _ => {}
        }
        None
    }

    /// The event still open when the stream ended, if it carried data.
    pub fn finish(&mut self) -> Option<Event> {
        let event = std::mem::take(&mut self.current);
        std::mem::take(&mut self.has_data).then_some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_end_at_blank_lines() {
        let mut lines = Lines::default();
        let mut out = Vec::new();
        for line in ["event: ping", "data: {\"a\":1}", "", ": comment", "data: x", "data: y", "", "data: [DONE]"] {
            out.extend(lines.push(line));
        }
        out.extend(lines.finish());
        assert_eq!(
            out,
            [
                Event { name: "ping".into(), data: "{\"a\":1}".into() },
                Event { name: String::new(), data: "x\ny".into() },
                Event { name: String::new(), data: "[DONE]".into() },
            ]
        );
    }
}
