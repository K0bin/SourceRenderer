use std::{fs::File, io::{BufWriter, Write}, sync::Mutex};

use log::{logger, Log};

struct LoggerFile {
    writer: BufWriter<File>,
    msg_count: u64
}

struct BuildScriptLogger {
    file: Mutex<LoggerFile>
}

impl Log for BuildScriptLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) { return; }
        println!("cargo::warning=\"{}\"", record.args());

        let do_flush = {
            let mut file = self.file.lock().unwrap();
            let _ = writeln!(&mut file.writer, "{}", record.args()).unwrap();
            file.msg_count += 1;
            file.msg_count % 8 == 0
        };

        if do_flush {
            self.flush();
        }
    }

    fn flush(&self) {
        let mut file = self.file.lock().unwrap();
        let _ = file.writer.flush().unwrap();
    }
}

impl Drop for BuildScriptLogger {
    fn drop(&mut self) {
        self.flush();
    }
}

pub fn init() {
    let file = File::create("build_script_output.txt").unwrap();
    let bufwriter = BufWriter::new(file);
    let boxed = Box::new(BuildScriptLogger{
        file: Mutex::new(LoggerFile {
            writer: bufwriter, msg_count: 0u64
        })
    });
    unsafe {
        let ptr = Box::into_raw(boxed);
        log::set_logger(&*ptr).unwrap();
        log::set_max_level(log::LevelFilter::Warn);
    }
}
