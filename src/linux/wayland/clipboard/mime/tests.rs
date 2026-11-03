use super::*;
use std::time::Duration;

#[test]
fn pipe_transfer_preserves_binary_bytes_and_requires_eof_within_bounds() {
    let mut pipe = Pipe::new().unwrap();
    pipe.write.write_all(b"\0\xff\r\n").unwrap();
    drop(pipe.write);
    assert_eq!(
        read_pipe(pipe.read, Instant::now() + Duration::from_secs(1), 4).unwrap(),
        b"\0\xff\r\n"
    );

    let mut pipe = Pipe::new().unwrap();
    pipe.write.write_all(b"abcde").unwrap();
    drop(pipe.write);
    assert!(
        matches!(read_pipe(pipe.read, Instant::now() + Duration::from_secs(1), 4), Err(ClipboardAccessError::Transfer(message)) if message.contains("byte limit"))
    );

    let pipe = Pipe::new().unwrap();
    assert!(
        matches!(read_pipe(pipe.read, Instant::now() + Duration::from_millis(10), 4), Err(ClipboardAccessError::Transfer(message)) if message.contains("timed out"))
    );
}
