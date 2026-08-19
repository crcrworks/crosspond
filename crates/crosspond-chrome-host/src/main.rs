use std::process::ExitCode;

fn main() -> ExitCode {
    let socket = crosspond_chrome_host::default_socket_path();
    if let Err(err) = crosspond_chrome_host::run_native_host(socket) {
        eprintln!("crosspond-chrome-host: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
