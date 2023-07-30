use crate::MultiProgress;
use log::Log;

/// Wraps a MultiProgress and a Log implementor
/// calling .suspend on the MultiProgress while writing the log message
/// thereby preventing progress bars and logs from getting mixed up.
///
/// You simply have to add all the progress bars in use to the MultiProgress in use.
pub struct LogWrapper<L: Log> {
    bar: MultiProgress,
    log: L,
}

impl<L: Log + 'static> LogWrapper<L> {
    pub fn new(bar: MultiProgress, log: L) -> Self {
        Self { bar, log }
    }

    /// installs this as the lobal logger,
    ///
    /// tries to find the correct argument to set_max_level
    /// by reading the logger configuration,
    /// you may want to set it manually though.
    pub fn try_init(self) -> Result<(), log::SetLoggerError> {
        use log::LevelFilter::*;
        let levels = [Off, Error, Warn, Info, Debug, Trace];

        for level_filter in levels.iter().rev() {
            let level = if let Some(level) = level_filter.to_level() {
                level
            } else {
                // off is the last level, just do nothing in that case
                continue;
            };
            let meta = log::Metadata::builder().level(level).build();
            if self.enabled(&meta) {
                log::set_max_level(*level_filter);
                break;
            }
        }

        log::set_boxed_logger(Box::new(self))
    }
    pub fn multi(&self) -> MultiProgress {
        self.bar.clone()
    }
}
impl<L: Log> Log for LogWrapper<L> {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.log.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        self.bar.suspend(|| self.log.log(record))
    }

    fn flush(&self) {
        self.log.flush()
    }
}
