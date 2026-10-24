mod executor;
mod native;
mod params;
mod server;
mod windows;

use rmcp::{transport::stdio, ServiceExt};
use server::Server;
use std::{ffi::OsString, process::ExitCode, time::Duration};

// The SDK drains in-flight responses before cancelling requests on EOF.
// Observe the byte stream so worker cancellation does not wait for that drain.
struct InputReader<R> {
    inner: R,
    ended: Option<tokio::sync::oneshot::Sender<bool>>,
}

impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for InputReader<R> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buffer.filled().len();
        let had_capacity = buffer.remaining() != 0;
        let result = std::pin::Pin::new(&mut this.inner).poll_read(context, buffer);
        let ended = match &result {
            std::task::Poll::Ready(Err(_)) => Some(false),
            std::task::Poll::Ready(Ok(())) if had_capacity && buffer.filled().len() == before => {
                Some(true)
            }
            _ => None,
        };
        if let Some(ended) = ended {
            if let Some(notify) = this.ended.take() {
                let _ = notify.send(ended);
            }
        }
        result
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Startup {
    Serve { allow_input: bool },
    Help,
    Version,
}
fn arguments(args: impl IntoIterator<Item = OsString>) -> Result<Startup, ()> {
    let mut args = args.into_iter();
    let first = args.next();
    if args.next().is_some() {
        return Err(());
    }
    match first.as_deref().and_then(|arg| arg.to_str()) {
        None if first.is_none() => Ok(Startup::Serve { allow_input: false }),
        Some("--allow-input") => Ok(Startup::Serve { allow_input: true }),
        Some("--help") => Ok(Startup::Help),
        Some("--version") => Ok(Startup::Version),
        _ => Err(()),
    }
}
fn main() -> ExitCode {
    let startup = match arguments(std::env::args_os().skip(1)) {
        Ok(startup) => startup,
        Err(()) => {
            eprintln!("invalid arguments; use --help");
            return ExitCode::from(2);
        }
    };
    let allow_input = match startup {
        Startup::Help => {
            println!("logi-vhid-mcp [--allow-input]\n\nLocal stdio MCP server. Input is disabled by default.\n  --allow-input  Enable input capability; the host must still obtain approval for each operation.\n  --help         Print this help and exit.\n  --version      Print the version and exit.\n\nInput affects the foreground desktop, not an isolated window. Submission does not prove receipt. Never automatically retry. Screenshots are supplied externally. Stop and shutdown attempt release, but cannot guarantee release after forced termination or blocked native calls.");
            return ExitCode::SUCCESS;
        }
        Startup::Version => {
            println!("logi-vhid-mcp {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Startup::Serve { allow_input } => allow_input,
    };
    // Build the runtime only for service mode; help and version never create a worker.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("cannot start async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let success = runtime.block_on(serve(allow_input));
    // A blocked synchronous API cannot be forcefully interrupted. Do not let
    // Tokio's blocking-query pool turn the bounded service shutdown into a hang.
    runtime.shutdown_timeout(Duration::from_millis(100));
    if success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
async fn serve(allow_input: bool) -> bool {
    let (reader, writer) = stdio();
    serve_with_io(Server::new(allow_input), reader, writer).await
}

async fn serve_with_io<R, W>(server: Server, reader: R, writer: W) -> bool
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
    W: tokio::io::AsyncWrite + Send + Unpin + 'static,
{
    let executor = server.executor();
    let (ended_tx, mut ended_rx) = tokio::sync::oneshot::channel();
    let reader = InputReader {
        inner: reader,
        ended: Some(ended_tx),
    };
    let mut protocol_ok = true;
    let service = tokio::select! {
        result=server.serve((reader, writer))=>Some(result),
        ended=&mut ended_rx=>{
            protocol_ok = matches!(ended, Ok(true));
            if !protocol_ok { eprintln!("stdin transport failed"); }
            None
        }
        signal=tokio::signal::ctrl_c()=>{
            if let Err(error)=signal { eprintln!("Ctrl+C listener failed: {error}"); protocol_ok=false; }
            None
        }
    };
    if let Some(service) = service {
        match service {
            Ok(service) => {
                let cancellation = service.cancellation_token();
                let waiting = service.waiting();
                tokio::pin!(waiting);
                tokio::select! {
                    result=&mut waiting=>{
                        if !matches!(result, Ok(rmcp::service::QuitReason::Closed | rmcp::service::QuitReason::Cancelled)) {
                            eprintln!("MCP service failed");protocol_ok=false;
                        }
                    }
                    ended=&mut ended_rx=>{
                        protocol_ok = matches!(ended, Ok(true));
                        if !protocol_ok { eprintln!("stdin transport failed"); }
                        cancellation.cancel();
                    }
                    signal=tokio::signal::ctrl_c()=>{
                        if let Err(error)=signal {eprintln!("Ctrl+C listener failed: {error}");protocol_ok=false;}
                        cancellation.cancel();
                    }
                }
            }
            Err(rmcp::service::ServerInitializeError::ConnectionClosed(_)) => {}
            Err(_) => {
                eprintln!("MCP startup failed");
                protocol_ok = false;
            }
        }
    }
    match tokio::time::timeout(Duration::from_secs(5), executor.shutdown()).await {
        Ok(Ok(())) => protocol_ok,
        Ok(Err(error)) => {
            eprintln!("input worker shutdown failed: {error}");
            false
        }
        Err(_) => {
            eprintln!("input worker shutdown timed out; release remains unconfirmed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_parser_accepts_only_the_four_supported_forms() {
        assert_eq!(arguments([]), Ok(Startup::Serve { allow_input: false }));
        assert_eq!(
            arguments(["--allow-input".into()]),
            Ok(Startup::Serve { allow_input: true })
        );
        assert_eq!(arguments(["--help".into()]), Ok(Startup::Help));
        assert_eq!(arguments(["--version".into()]), Ok(Startup::Version));
        for args in [
            vec!["--allow-input", "--allow-input"],
            vec!["--help", "--version"],
            vec!["stdio"],
            vec!["--unknown"],
        ] {
            assert!(arguments(args.into_iter().map(OsString::from)).is_err());
        }
    }

    #[tokio::test]
    async fn zero_capacity_read_is_not_misreported_as_stdin_eof() {
        use tokio::io::AsyncReadExt;
        let (ended_tx, mut ended_rx) = tokio::sync::oneshot::channel();
        let mut reader = InputReader {
            inner: tokio::io::empty(),
            ended: Some(ended_tx),
        };
        assert_eq!(reader.read(&mut []).await.unwrap(), 0);
        assert!(matches!(
            ended_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert_eq!(reader.read(&mut [0]).await.unwrap(), 0);
        assert!(ended_rx.await.unwrap());
    }
}
