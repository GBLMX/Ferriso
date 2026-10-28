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
use std::future::Future;
use std::pin::Pin;

struct MprisTask {
    connection: zbus::Connection,
}

impl Sidecar for MprisTask {
    fn shutdown(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            // Dropping the connection removes the well-known name and objects.
            drop(self.connection);
        })
    }
}

fn start_mpris(context: CoreContext<Message, View>) -> Sidecars {
    let handle = context.handle().clone();
    let task = tokio::spawn(async move {
        let connection = zbus::connection::Builder::session()
            .expect("session bus")
            .name("org.mpris.MediaPlayer2.example")
            .expect("request MPRIS name")
            .serve_at("/org/mpris/MediaPlayer2", Player { handle })
            .expect("serve MPRIS object")
            .build()
            .await
            .expect("start MPRIS service");
        // Keep the service task alive for the actor's lifetime.
        let _ = connection;
    });

    let mut sidecars = Sidecars::new();
    sidecars.push(task);
    sidecars
}

let mut sidecars = start_mpris(context);
// A D-Bus `Quit` method can request the same clean stop the CLI uses:
// handle.request_exit();
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
