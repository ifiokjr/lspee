#![cfg_attr(not(unix), allow(unused))]

#[cfg(not(unix))]
compile_error!("lspee currently supports unix-like platforms only (linux/macOS)");

pub mod commands;
