//! A minimal Ferriso application rendered by GPUI.

use ferriso_core::{App, Command};
use ferriso_gpui::{Renderer, Settings, Ui, run};
use gpui::{AnyElement, App as GpuiApp, IntoElement, Window, div, prelude::*};

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
    fn render(
        &mut self,
        value: &i64,
        ui: &Ui<Message>,
        _window: &mut Window,
        _cx: &mut GpuiApp,
    ) -> AnyElement {
        let increment = ui.clone();
        let decrement = ui.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(gpui::white())
            .text_color(gpui::black())
            .child(format!("Value: {value}"))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(action("Increment", move || {
                        increment.send(Message::Add(1));
                    }))
                    .child(action("Decrement", move || {
                        decrement.send(Message::Add(-1));
                    })),
            )
            .into_any_element()
    }
}

fn action(label: &'static str, on_click: impl Fn() + 'static) -> impl IntoElement {
    div()
        .id(label)
        .cursor_pointer()
        .px_3()
        .py_2()
        .bg(gpui::rgb(0xf2f2f2))
        .border_1()
        .border_color(gpui::black())
        .rounded_md()
        .child(label)
        .on_click(move |_, _, _| on_click())
}

fn main() {
    run(Counter(0), CounterView, Settings::default());
}
