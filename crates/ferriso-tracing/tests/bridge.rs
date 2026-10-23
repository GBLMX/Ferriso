use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

use ferriso_diagnostics::{Diagnostics, Level, TaskId};
use ferriso_tracing::TracingLogger;
use tracing_subscriber::fmt::MakeWriter;

#[derive(Clone, Default)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("buffer lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Buffer {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn diagnostics_records_become_structured_tracing_events() {
    let buffer = Buffer::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(buffer.clone())
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        let diagnostics = Diagnostics::new()
            .with_debug(true)
            .with_logger(Arc::new(TracingLogger::new()));

        diagnostics.log(
            Level::Info,
            "ferriso-test-target",
            "bridge-test",
            TaskId::NONE,
            "hello tracing",
        );
    });

    let output = String::from_utf8(buffer.0.lock().expect("buffer lock").clone())
        .expect("tracing output is utf-8");
    assert!(output.contains("ferriso-test-target"), "{output}");
    assert!(output.contains("bridge-test"), "{output}");
    assert!(output.contains("hello tracing"), "{output}");
}
