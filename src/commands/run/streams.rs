use std::io::{self, BufRead, BufReader, Read, Write};
use std::thread::{self, JoinHandle};

const RECORD_LIMIT: u64 = 8192;

pub(super) type Reader = JoinHandle<io::Result<()>>;

pub(super) fn start(
    name: &str,
    input: impl Read + Send + 'static,
    stderr: bool,
) -> io::Result<Reader> {
    let label = format!("[{name}] ");
    thread::Builder::new().spawn(move || {
        let mut input = BufReader::new(input);
        let mut record = Vec::with_capacity(RECORD_LIMIT as usize);
        loop {
            record.clear();
            if input
                .by_ref()
                .take(RECORD_LIMIT)
                .read_until(b'\n', &mut record)?
                == 0
            {
                return Ok(());
            }
            if stderr {
                write_record(io::stderr().lock(), &label, &record)?;
            } else {
                write_record(io::stdout().lock(), &label, &record)?;
            }
        }
    })
}

fn write_record(mut output: impl Write, label: &str, record: &[u8]) -> io::Result<()> {
    output.write_all(label.as_bytes())?;
    output.write_all(record)?;
    if !record.ends_with(b"\n") {
        output.write_all(b"\n")?;
    }
    Ok(())
}

pub(super) fn finish(readers: Vec<Reader>) -> io::Result<()> {
    let mut result = Ok(());
    for reader in readers {
        let joined = reader
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("task output reader panicked")));
        result = result.and(joined);
    }
    result
}
