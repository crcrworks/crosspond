use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--install") {
        return install_manifests();
    }
    let socket = crosspond_chrome_host::default_socket_path();
    if let Err(err) = crosspond_chrome_host::run_native_host(socket) {
        eprintln!("crosspond-chrome-host: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn install_manifests() -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("crosspond-chrome-host: {err}");
            return ExitCode::FAILURE;
        }
    };
    match crosspond_chrome_host::install_native_host_manifests(&exe) {
        Ok(paths) => {
            for path in paths {
                println!("{}", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("crosspond-chrome-host: {err}");
            ExitCode::FAILURE
        }
    }
}
