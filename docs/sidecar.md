# Sidecars: IPC, MPRIS, and other long-lived services

A sidecar is a service that lives beside the actor and needs the same actor
lifetime. Typical examples are a Unix socket for CLI control and an MPRIS
object on the session bus. Start them through
`run_with_app_services` (TUI or GUI); the adapter shuts them down before it asks
the actor to stop.

The runnable Unix-socket example lives at
`crates/ferriso-tui/examples/sidecar.rs`:

```sh
cargo run -p ferriso-tui --example sidecar
```

It demonstrates the two framework pieces:

- `CoreContext` supplies the actor `handle` and `snapshots` after startup.
- `Sidecars` owns the service task and stops it when the UI exits.

## MPRIS sketch

Add `zbus` to the application crate, then serve the MPRIS object inside the
same hook. The exact interface bodies depend on the application's transport
messages; the lifecycle is the part Ferriso owns.

```rust,ignore
use ferriso_core::{CoreContext, Sidecar, Sidecars};
use std::{future::Future, pin::Pin};
use tokio::{sync::oneshot, task::JoinHandle};

struct MprisTask {
    task: JoinHandle<()>,
    stop: oneshot::Sender<()>,
}

impl Sidecar for MprisTask {
    fn shutdown(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            // The task owns the connection and drops it after receiving this.
            let _ = self.stop.send(());
            let _ = self.task.await;
        })
    }
}

fn start_mpris(context: CoreContext<Message, View>) -> Sidecars {
    let handle = context.handle().clone();
    let (stop, stop_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let connection = match zbus::connection::Builder::session()
            .expect("session bus")
            .name("org.mpris.MediaPlayer2.example")
            .expect("request MPRIS name")
            .serve_at("/org/mpris/MediaPlayer2", Player { handle })
            .expect("serve MPRIS object")
            .build()
            .await
        {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("MPRIS unavailable: {error}");
                return;
            }
        };
        // Keep the connection alive until the sidecar asks the task to stop.
        let _keep_alive = connection;
        let _ = stop_rx.await;
    });

    let mut sidecars = Sidecars::new();
    sidecars.push(MprisTask { task, stop });
    sidecars
}
```

Wire it through the adapter hook:

```rust,ignore
ferriso_tui::run_with_app_services(
    || App::new(),
    start_mpris,
    AppTui::new(),
    ferriso_tui::Options::default(),
)?;
```

`Player` above is the application's `#[zbus::interface]` type. Its methods
translate D-Bus calls into application messages with `handle.send(...)`; a
`Quit` method calls `handle.request_exit()`. This is exactly the same path as
`CoreContext::request_exit`, so TUI and GUI close through their existing
actor-closed handling.

## Ordering

`run_with_app_services` does this, in order:

1. build the app on the adapter runtime;
2. spawn the actor;
3. call the startup hook with `CoreContext`;
4. run the terminal or window;
5. stop the returned `Sidecars`;
6. request actor shutdown and join it.

If a service needs an asynchronous flush, implement `Sidecar` for a small
wrapper and do that flush in `Sidecar::shutdown`. Keep the future bounded;
the adapter does not impose a timeout on user services.
