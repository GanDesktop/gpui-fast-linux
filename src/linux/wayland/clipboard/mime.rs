use super::*;
use std::{io::Read, time::Instant};

impl Clipboard {
    pub fn check_offer(&self, expected: Option<u64>) -> Result<(), ClipboardAccessError> {
        if expected.is_some_and(|id| id != self.generation) {
            Err(ClipboardAccessError::Changed)
        } else {
            Ok(())
        }
    }

    pub fn set_mime(&mut self, entries: Arc<Vec<ClipboardMime>>) {
        self.contents = None;
        self.mime_contents = Some(entries);
        self.cached_read = None;
        self.owned = true;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn clear_mime(&mut self) {
        self.contents = None;
        self.mime_contents = None;
        self.current_offer = None;
        self.cached_read = None;
        self.owned = false;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn read_mime(
        &self,
        types: Vec<String>,
        limit: usize,
        executor: &BackgroundExecutor,
    ) -> Task<Result<Option<ClipboardMimeOffer>, ClipboardAccessError>> {
        let id = self.generation;
        if self.owned {
            let entries = if let Some(entries) = &self.mime_contents {
                entries
                    .iter()
                    .filter(|entry| types.contains(&entry.mime_type))
                    .cloned()
                    .collect()
            } else if let Some(item) = &self.contents {
                let mut entries = Vec::new();
                if let Some(text) = item.text() {
                    for mime in types
                        .iter()
                        .filter(|mime| TEXT_MIME_TYPES.contains(&mime.as_str()))
                    {
                        entries.push(ClipboardMime {
                            mime_type: mime.clone(),
                            bytes: text.as_bytes().to_vec(),
                        });
                    }
                }
                for entry in &item.entries {
                    if let ClipboardEntry::Image(image) = entry {
                        let mime = image.format.mime_type();
                        if types.iter().any(|requested| requested == mime) {
                            entries.push(ClipboardMime {
                                mime_type: mime.into(),
                                bytes: image.bytes.clone(),
                            });
                        }
                    }
                }
                entries
            } else {
                Vec::new()
            };
            return Task::ready(
                if entries.iter().map(|entry| entry.bytes.len()).sum::<usize>() > limit {
                    Err(ClipboardAccessError::Transfer(
                        "clipboard payload exceeds the byte limit".into(),
                    ))
                } else {
                    Ok(Some(ClipboardMimeOffer { id, entries }))
                },
            );
        }
        let Some(offer) = &self.current_offer else {
            return Task::ready(Ok(None));
        };
        // Queue all receives against this exact offer before returning to the event loop.
        let pipes = (|| {
            let mut pipes = Vec::new();
            for mime in types {
                if !offer.has_mime_type(&mime)
                    || pipes.iter().any(|(existing, _)| existing == &mime)
                {
                    continue;
                }
                let pipe = Pipe::new().map_err(transfer_error)?;
                offer.inner.receive_data(mime.clone(), unsafe {
                    BorrowedFd::borrow_raw(pipe.write.as_raw_fd())
                });
                drop(pipe.write);
                pipes.push((mime, pipe.read));
            }
            self.connection.flush().map_err(transfer_error)?;
            Ok::<_, ClipboardAccessError>(pipes)
        })();
        let deadline = Instant::now() + PIPE_READ_TIMEOUT;
        executor.spawn(async move {
            let mut remaining = limit;
            let mut entries = Vec::new();
            for (mime_type, pipe) in pipes? {
                let bytes = read_pipe(pipe, deadline, remaining)?;
                remaining -= bytes.len();
                entries.push(ClipboardMime { mime_type, bytes });
            }
            Ok(Some(ClipboardMimeOffer { id, entries }))
        })
    }
}

fn transfer_error(error: impl std::fmt::Display) -> ClipboardAccessError {
    ClipboardAccessError::Transfer(error.to_string())
}

fn read_pipe(
    mut fd: filedescriptor::FileDescriptor,
    deadline: Instant,
    limit: usize,
) -> Result<Vec<u8>, ClipboardAccessError> {
    fd.set_non_blocking(true).map_err(transfer_error)?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let timeout = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| ClipboardAccessError::Transfer("clipboard source timed out".into()))?;
        let mut fds = [filedescriptor::pollfd {
            fd: fd.as_raw_fd(),
            events: filedescriptor::POLLIN,
            revents: 0,
        }];
        match filedescriptor::poll(&mut fds, Some(timeout)) {
            Ok(0) => {
                return Err(ClipboardAccessError::Transfer(
                    "clipboard source timed out".into(),
                ));
            }
            Ok(_) => (),
            Err(filedescriptor::Error::Poll(error)) if error.kind() == ErrorKind::Interrupted => {
                continue;
            }
            Err(error) => return Err(transfer_error(error)),
        }
        match fd.read(&mut chunk) {
            Ok(0) => return Ok(buffer),
            Ok(n) => {
                if n > limit.saturating_sub(buffer.len()) {
                    return Err(ClipboardAccessError::Transfer(
                        "clipboard payload exceeds the byte limit".into(),
                    ));
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
            {
                ()
            }
            Err(error) => return Err(transfer_error(error)),
        }
    }
}

#[cfg(test)]
mod tests;
