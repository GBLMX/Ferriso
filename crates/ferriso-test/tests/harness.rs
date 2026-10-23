use std::num::NonZeroUsize;
use std::time::Duration;

use ferriso_core::{App, Command, RuntimeOptions};
use ferriso_test::Harness;
use tokio::time::timeout;

#[derive(Default)]
struct Counter {
    count: u32,
}

enum Message {
    Add(u32),
}

impl App for Counter {
    type Message = Message;
    type View = u32;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Add(value) => self.count += value,
        }
        Command::none()
    }

    fn view(&self) -> u32 {
        self.count
    }
}

#[tokio::test]
async fn sends_messages_and_waits_for_a_view_predicate() {
    let options = RuntimeOptions::new(NonZeroUsize::new(1).expect("non-zero capacity"));
    let mut harness = Harness::with_options(Counter::default(), options);

    assert_eq!(*harness.view(), 0);
    assert!(harness.send(Message::Add(2)));

    let view = timeout(Duration::from_secs(1), harness.wait_for(|view| *view == 2))
        .await
        .expect("predicate arrives")
        .expect("actor alive");
    assert_eq!(*view, 2);

    harness.shutdown().await;
}

#[tokio::test]
async fn wait_for_returns_none_after_the_actor_closes() {
    let mut harness = Harness::new(Counter::default());
    harness.handle().shutdown();

    let view = timeout(Duration::from_secs(1), harness.wait_for(|_| false))
        .await
        .expect("actor closes");
    assert!(view.is_none());

    assert!(harness.closed());
    harness.shutdown().await;
}

#[tokio::test]
async fn dropping_the_harness_releases_the_actor() {
    let harness = Harness::new(Counter::default());
    let snapshots = harness.snapshots();
    drop(harness);

    timeout(Duration::from_secs(1), async {
        while !snapshots.closed() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("actor exits after harness drop");
}
