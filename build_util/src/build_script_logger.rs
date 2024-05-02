use log::Log;

struct BuildScriptLogger {}

impl Log for BuildScriptLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) { return; }
        println!("cargo::warning=\"{}\"", record.args());
    }

    fn flush(&self) {}
}

pub fn init() {
    let boxed = Box::new(BuildScriptLogger{});
    unsafe {
        let ptr = Box::into_raw(boxed);
        log::set_logger(&*ptr).unwrap();
        log::set_max_level(log::LevelFilter::Warn);
    }
}
