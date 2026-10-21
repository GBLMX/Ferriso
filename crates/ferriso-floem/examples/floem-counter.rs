//! A minimal Ferriso application rendered by Floem.

use ferriso_core::{App, Command};
use ferriso_floem::{Renderer, Settings, Ui, run};
use floem::{
    AnyView,
    prelude::*,
    views::{Decorators, button, h_stack, label, v_stack},
};

struct Counter(i64);

#[derive(Clone, Copy)]
enum Message {
    Add(i64),
}

impl App for Counter {
    type Message = Message;
    type View = i64;

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Add(delta) => self.0 += delta,
        }
        Command::none()
    }

    fn view(&self) -> i64 {
        self.0
    }
}

struct CounterView;

impl Renderer<i64, Message> for CounterView {
    fn view(&self, value: &i64, ui: &Ui<Message>) -> AnyView {
        let increment = ui.clone();
        let decrement = ui.clone();
        let value = *value;

        v_stack((
            label(move || format!("Value: {value}")),
            h_stack((
                button("Increment").action(move || {
                    increment.send(Message::Add(1));
                }),
                button("Decrement").action(move || {
                    decrement.send(Message::Add(-1));
                }),
            )),
        ))
        .style(|style| {
            style
                .size_full()
                .items_center()
                .justify_center()
                .row_gap(12.0)
                .font_size(20.0)
        })
        .into_any()
    }
}

fn main() {
    run(Counter(0), CounterView, Settings::default());
}
