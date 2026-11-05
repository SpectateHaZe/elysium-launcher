//! Game log parsing. Versions using Mojang's log4j config print XML events on stdout;
//! those are turned back into readable lines. Plain output is passed through.

use chrono::{Local, TimeZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogSource {
    Game,
    Launcher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub level: LogLevel,
    pub source: LogSource,
    pub text: String,
}

impl LogLine {
    pub fn launcher(level: LogLevel, text: impl Into<String>) -> Self {
        Self {
            level,
            source: LogSource::Launcher,
            text: text.into(),
        }
    }
}

/// Stateful line parser (an XML event spans several lines).
#[derive(Debug, Default)]
pub struct LogParser {
    event: Option<String>,
}

impl LogParser {
    pub fn feed(&mut self, line: &str) -> Vec<LogLine> {
        if let Some(buf) = &mut self.event {
            buf.push('\n');
            buf.push_str(line);
            if line.contains("</log4j:Event>") {
                let event = self.event.take().unwrap_or_default();
                return parse_event(&event);
            }
            return Vec::new();
        }
        if let Some(start) = line.find("<log4j:Event") {
            let rest = &line[start..];
            if rest.contains("</log4j:Event>") {
                return parse_event(rest);
            }
            self.event = Some(rest.to_owned());
            return Vec::new();
        }
        vec![plain(line)]
    }

    /// Emits whatever is buffered (e.g. a truncated event when the game crashed).
    pub fn finish(&mut self) -> Vec<LogLine> {
        self.event
            .take()
            .map(|e| vec![plain(&e)])
            .unwrap_or_default()
    }
}

fn plain(line: &str) -> LogLine {
    LogLine {
        level: guess_level(line),
        source: LogSource::Game,
        text: line.to_owned(),
    }
}

fn guess_level(line: &str) -> LogLevel {
    let upper_has = |pats: &[&str]| pats.iter().any(|p| line.contains(p));
    if upper_has(&[
        "/ERROR]",
        "/FATAL]",
        "[SEVERE]",
        "Exception",
        "\tat ",
        "Caused by:",
    ]) || line.trim_start().starts_with("at ")
    {
        LogLevel::Error
    } else if upper_has(&["/WARN]", "[WARNING]", "[WARN]"]) {
        LogLevel::Warn
    } else if upper_has(&["/DEBUG]", "/TRACE]"]) {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

fn attr<'a>(event: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = event.find(&key)? + key.len();
    let len = event[start..].find('"')?;
    Some(&event[start..start + len])
}

fn cdata_in<'a>(event: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<log4j:{tag}>");
    let body = &event[event.find(&open)? + open.len()..];
    let body = &body[..body.find(&format!("</log4j:{tag}>"))?];
    let body = body.trim();
    Some(
        body.strip_prefix("<![CDATA[")
            .and_then(|b| b.strip_suffix("]]>"))
            .unwrap_or(body),
    )
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn parse_event(event: &str) -> Vec<LogLine> {
    let level = match attr(event, "level").unwrap_or("INFO") {
        "ERROR" | "FATAL" => LogLevel::Error,
        "WARN" => LogLevel::Warn,
        "DEBUG" | "TRACE" => LogLevel::Debug,
        _ => LogLevel::Info,
    };
    let thread = unescape(attr(event, "thread").unwrap_or("main"));
    let time = attr(event, "timestamp")
        .and_then(|t| t.parse::<i64>().ok())
        .and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .map(|t| t.format("%H:%M:%S").to_string())
        .unwrap_or_default();
    let message = cdata_in(event, "Message").unwrap_or_default();
    let prefix = format!(
        "[{time}] [{thread}/{}]: ",
        attr(event, "level").unwrap_or("INFO")
    );

    let mut lines = Vec::new();
    for (i, text) in message.lines().enumerate() {
        let text = if i == 0 {
            format!("{prefix}{text}")
        } else {
            text.to_owned()
        };
        lines.push(LogLine {
            level,
            source: LogSource::Game,
            text,
        });
    }
    if lines.is_empty() {
        lines.push(LogLine {
            level,
            source: LogSource::Game,
            text: prefix,
        });
    }
    if let Some(throwable) = cdata_in(event, "Throwable") {
        lines.extend(throwable.lines().map(|t| LogLine {
            level: LogLevel::Error,
            source: LogSource::Game,
            text: t.to_owned(),
        }));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_xml_event() {
        let mut p = LogParser::default();
        assert!(p.feed(r#"<log4j:Event logger="ewp" timestamp="1700000000000" level="WARN" thread="Render thread">"#).is_empty());
        assert!(
            p.feed("  <log4j:Message><![CDATA[Hello <world>\nsecond]]></log4j:Message>")
                .is_empty()
        );
        let out = p.feed("</log4j:Event>");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].level, LogLevel::Warn);
        assert!(
            out[0].text.ends_with("[Render thread/WARN]: Hello <world>"),
            "{}",
            out[0].text
        );
        assert_eq!(out[1].text, "second");
    }

    #[test]
    fn parses_throwable() {
        let mut p = LogParser::default();
        let out = p.feed(r#"<log4j:Event logger="a" timestamp="1" level="ERROR" thread="main"><log4j:Message><![CDATA[boom]]></log4j:Message><log4j:Throwable><![CDATA[java.lang.RuntimeException
	at a.b(C.java:1)]]></log4j:Throwable></log4j:Event>"#);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|l| l.level == LogLevel::Error));
    }

    #[test]
    fn plain_lines_and_truncated_events() {
        let mut p = LogParser::default();
        assert_eq!(
            p.feed("[12:00:00] [Client thread/WARN]: careful")[0].level,
            LogLevel::Warn
        );
        assert_eq!(p.feed("Setting user: Steve")[0].level, LogLevel::Info);
        assert_eq!(p.feed("\tat net.minecraft.X")[0].level, LogLevel::Error);
        assert!(p.feed("<log4j:Event level=\"INFO\">").is_empty());
        assert_eq!(p.finish().len(), 1);
    }
}
