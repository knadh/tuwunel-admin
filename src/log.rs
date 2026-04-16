macro_rules! info {
    ($($arg:tt)*) => { eprintln!("[info]  {}", format_args!($($arg)*)) };
}

macro_rules! debug {
    ($($arg:tt)*) => {
        if std::env::var_os("TUWADM_DEBUG").is_some() {
            eprintln!("[debug] {}", format_args!($($arg)*));
        }
    };
}

macro_rules! error {
    ($($arg:tt)*) => { eprintln!("[error] {}", format_args!($($arg)*)) };
}

pub(crate) use {debug, error, info};
