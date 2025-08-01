use std::time::{SystemTime, UNIX_EPOCH};

pub fn current_unix_timestamp_in_ms() -> u64 {
    // https://stackoverflow.com/questions/26593387/how-can-i-get-the-current-time-in-milliseconds
    let time = SystemTime::now();
    let unix_timestamp = time
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward");

    unix_timestamp.as_secs() * 1000 + unix_timestamp.subsec_nanos() as u64 / 1_000_000
}
