use std::{
    fs::{self, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    process,
    sync::Mutex,
};

use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use crate::state::ChatMessage;

const TRANSCRIPTS_DIR: &str = ".castra/transcripts";

pub struct TranscriptWriter {
    session_id: String,
    path: PathBuf,
    inner: Mutex<TranscriptInner>,
}

struct TranscriptInner {
    writer: BufWriter<std::fs::File>,
    sequence: u64,
}

#[derive(Serialize)]
struct TranscriptRecord<'a> {
    session_id: &'a str,
    sequence: u64,
    recorded_at: &'a str,
    display_timestamp: &'a str,
    speaker: &'a str,
    kind: &'a str,
    text: &'a str,
    expanded_by_default: bool,
}

impl TranscriptWriter {
    pub fn new(workspace_root: &Path) -> io::Result<Self> {
        let session_id = generate_session_id();
        let path = build_transcript_path(workspace_root, &session_id)?;
        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        Ok(Self {
            session_id,
            path,
            inner: Mutex::new(TranscriptInner {
                writer: BufWriter::new(file),
                sequence: 0,
            }),
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record(&self, message: &ChatMessage) -> io::Result<()> {
        let recorded_at = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
        let display_timestamp = message.timestamp().to_string();
        let speaker = message.speaker().to_string();
        let text = message.text().to_string();
        let kind = message.kind().slug();
        let expanded_by_default = message.is_expanded();

        let mut inner = self.inner.lock().expect("transcript writer poisoned");
        inner.sequence += 1;

        let record = TranscriptRecord {
            session_id: &self.session_id,
            sequence: inner.sequence,
            recorded_at: &recorded_at,
            display_timestamp: &display_timestamp,
            speaker: &speaker,
            kind,
            text: &text,
            expanded_by_default,
        };

        serde_json::to_writer(&mut inner.writer, &record)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        inner.writer.write_all(b"\n")?;
        inner.writer.flush()?;
        Ok(())
    }
}

fn build_transcript_path(workspace_root: &Path, session_id: &str) -> io::Result<PathBuf> {
    let dir = workspace_root.join(TRANSCRIPTS_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("chat-{session_id}.jsonl")))
}

fn generate_session_id() -> String {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let pid = process::id();
    format!("{timestamp}-{pid}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn writes_json_lines_records() {
        let temp = tempdir().unwrap();
        let workspace_root = temp.path();

        let writer = TranscriptWriter::new(workspace_root).expect("writer");
        let path = writer.path().to_path_buf();
        let mut message = ChatMessage::new("SYSTEM", "hello world");
        assert!(message.is_expanded());

        writer.record(&message).expect("record succeeds");

        message = ChatMessage::new("CODEX·CMD", "cargo test");
        writer.record(&message).expect("record succeeds");

        drop(writer);

        let contents = fs::read_to_string(path).expect("transcript exists");
        let mut lines = contents.lines();

        let first: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        let second: Value = serde_json::from_str(lines.next().unwrap()).unwrap();

        assert_eq!(first["sequence"], 1);
        assert_eq!(second["sequence"], 2);
        assert!(first["display_timestamp"].as_str().is_some());
        assert_eq!(first["kind"], "system");
        assert_eq!(second["kind"], "tool");
        assert_eq!(first["expanded_by_default"], true);
        assert_eq!(second["expanded_by_default"], false);
        assert_eq!(first["session_id"], second["session_id"]);
    }
}
