use crate::HarnessError;
use crate::events::ThreadEvent;

pub fn decode_line(line: &str) -> Result<ThreadEvent, HarnessError> {
    Ok(serde_json::from_str(line.trim())?)
}
