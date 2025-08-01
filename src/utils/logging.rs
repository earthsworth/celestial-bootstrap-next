
#[macro_export]
macro_rules! log_backtrace {
    // log_backtrace!(logger: my_logger, target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // log_backtrace!(logger: my_logger, target: "my_target", "a {} event", "log")
    (logger: $logger:expr, target: $target:expr, $($arg:tt)+) => ({
        log::log!(logger: log::__log_logger!($logger), target: $target, log::Level::Error, $($arg)+);
        log::log!(logger: log::__log_logger!($logger), target: $target, log::Level::Error, "Backtrace: \n{}", std::backtrace::Backtrace::capture());
    });

    // log_backtrace!(logger: my_logger, key1 = 42, key2 = true; "a {} event", "log")
    // log_backtrace!(logger: my_logger, "a {} event", "log")
    (logger: $logger:expr, $($arg:tt)+) => ({
        log::log!(logger: log::__log_logger!($logger), log::Level::Error, $($arg)+);
        log::log!(logger: log::__log_logger!($logger), log::Level::Error, "Backtrace: \n{}", std::backtrace::Backtrace::capture());
    });

    // log_backtrace!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // log_backtrace!(target: "my_target", "a {} event", "log")
    (target: $target:expr, $($arg:tt)+) => ({
        log::log!(target: $target, log::Level::Error, $($arg)+);
        log::log!(target: $target, log::Level::Error, "Backtrace: \n{}", std::backtrace::Backtrace::capture());
    });

    // log_backtrace!("a {} event", "log")
    ($($arg:tt)+) => ({
        log::log!(log::Level::Error, $($arg)+);
        log::log!(log::Level::Error, "Backtrace: \n{}", std::backtrace::Backtrace::capture());
    })
}