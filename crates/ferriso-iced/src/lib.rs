//! Iced backend for Ferriso.
//!
//! Iced owns layout, text editing, and focus. This crate maps the [`ferriso::Node`] tree to
//! native widgets and folds framework events back into the application reducer.

use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Mutex,
};

use ferriso::node::{
    Align, ButtonKind, ImageData, ImageFit, Length, ListItem, Node, Padding, Surface, Weight,
};
use ferriso::theme::Tone;
use ferriso::{App, Command, Key, KeyEvent, Modifiers, Theme as FrTheme};
use iced::{
    Alignment, Border, Color, ContentFit, Element, Length as ILength, Subscription, Task,
    Theme,
    alignment::{Horizontal, Vertical},
    keyboard::key::Named,
    widget::{
        self, Column, Row, button, container, image, progress_bar, rule, scrollable, slider,
        space, text, text_input,
    },
};

// Bytes for one image, keyed by the node's cache id. Iced handles are reference-counted; the
// cache prevents re-uploading the same cover every frame.
thread_local! {
    static IMAGES: RefCell<HashMap<u64, image::Handle>> = RefCell::new(HashMap::new());
    static QRS: RefCell<HashMap<u64, image::Handle>> = RefCell::new(HashMap::new());
}

pub use iced::Result;

type El<'a, M> = Element<'a, Event<M>>;

#[derive(Clone, Debug)]
pub enum Event<Message> {
    User(Message),
    Tick,
    Key(KeyEvent),
}

pub struct Settings {
    pub title: Option<String>,
    pub size: Option<(u32, u32)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            title: None,
            size: None,
        }
    }
}

pub fn run<A>(app: A) -> iced::Result
where
    A: App + Send,
{
    run_with(app, Settings::default())
}

pub fn run_with<A>(app: A, settings: Settings) -> iced::Result
where
    A: App + Send,
{
    let size = settings.size.unwrap_or_else(|| app.window_size());
    let state = Mutex::new(Some(app));

    iced::application(
        move || {
            let mut app = state
                .lock()
                .expect("ferriso boot lock")
                .take()
                .expect("ferriso boot is called once");
            let command = app.init();
            (app, task(command))
        },
        update::<A>,
        view::<A>,
    )
    .title(move |app: &A| {
        settings
            .title
            .clone()
            .unwrap_or_else(|| app.title().to_string())
    })
    .theme(|app: &A| iced_theme(&app.theme()))
    .subscription(|app: &A| subscriptions::<A>(app))
    .window_size(size)
    .antialiasing(true)
    .run()
}

fn view<A: App>(app: &A) -> Element<'_, Event<A::Message>> {
    build(app.view(), &app.theme())
}

fn update<A: App>(app: &mut A, event: Event<A::Message>) -> Task<Event<A::Message>> {
    let command = match event {
        Event::User(message) => app.update(message),
        Event::Tick => app.tick(),
        Event::Key(key) => app.on_key(key),
    };
    let task = task(command);
    if app.should_quit() {
        Task::batch([task, iced::exit()])
    } else {
        task
    }
}

fn task<Message: Send + 'static>(command: Command<Message>) -> Task<Event<Message>> {
    Task::batch(
        command
            .into_futures()
            .map(|future| Task::perform(future, Event::User)),
    )
}

fn subscriptions<A: App>(app: &A) -> Subscription<Event<A::Message>> {
    let mut subscriptions = vec![
        iced::keyboard::listen().filter_map(|event| map_key(event).map(Event::Key)),
    ];
    if let Some(interval) = app.tick_interval() {
        subscriptions.push(iced::time::every(interval).map(|_| Event::Tick));
    }
    Subscription::batch(subscriptions)
}

fn map_key(event: iced::keyboard::Event) -> Option<KeyEvent> {
    match event {
        iced::keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            repeat,
            ..
        } => Some(map_key_fields(key, modifiers, true, repeat, text)),
        iced::keyboard::Event::KeyReleased {
            key, modifiers, ..
        } => Some(map_key_fields(key, modifiers, false, false, None)),
        _ => None,
    }
}

fn map_key_fields(
    key: iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    pressed: bool,
    repeat: bool,
    text: Option<iced::keyboard::SmolStr>,
) -> KeyEvent {
    let key = match key {
        iced::keyboard::Key::Character(value) => Key::Character(value.to_string()),
        iced::keyboard::Key::Named(named) => match named {
            Named::Space => Key::Space,
            Named::Enter => Key::Enter,
            Named::Tab => Key::Tab,
            Named::Escape => Key::Escape,
            Named::Backspace => Key::Backspace,
            Named::Delete => Key::Delete,
            Named::Insert => Key::Insert,
            Named::ArrowLeft => Key::Left,
            Named::ArrowRight => Key::Right,
            Named::ArrowUp => Key::Up,
            Named::ArrowDown => Key::Down,
            Named::Home => Key::Home,
            Named::End => Key::End,
            Named::PageUp => Key::PageUp,
            Named::PageDown => Key::PageDown,
            _ => Key::Other,
        },
        _ => Key::Other,
    };
    KeyEvent {
        key,
        modifiers: Modifiers {
            control: modifiers.control(),
            shift: modifiers.shift(),
            alt: modifiers.alt(),
            logo: modifiers.logo(),
        },
        pressed,
        repeat,
        text: text.map(|text| text.to_string()),
    }
}

fn color(rgb: ferriso::Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

fn iced_theme(theme: &FrTheme) -> Theme {
    Theme::custom(
        theme.name,
        iced::theme::Palette {
            background: color(theme.background),
            text: color(theme.text),
            primary: color(theme.accent),
            success: color(theme.success),
            warning: color(theme.warning),
            danger: color(theme.danger),
        },
    )
}

fn length(length: Length) -> ILength {
    match length {
        Length::Shrink => ILength::Shrink,
        Length::Fill => ILength::Fill,
        Length::Fixed(value) => ILength::Fixed(f32::from(value)),
        Length::FillPortion(portion) => ILength::FillPortion(portion),
    }
}

fn padding(padding: Padding) -> iced::Padding {
    iced::Padding {
        top: f32::from(padding.top),
        right: f32::from(padding.right),
        bottom: f32::from(padding.bottom),
        left: f32::from(padding.left),
    }
}

fn horizontal(align: Align) -> Horizontal {
    match align {
        Align::Start => Horizontal::Left,
        Align::Center => Horizontal::Center,
        Align::End => Horizontal::Right,
    }
}

fn vertical(align: Align) -> Vertical {
    match align {
        Align::Start => Vertical::Top,
        Align::Center => Vertical::Center,
        Align::End => Vertical::Bottom,
    }
}

fn build<'a, Message: Clone + 'static>(node: Node<'_, Message>, theme: &FrTheme) -> El<'a, Message> {
    build_with(node, theme, None)
}

fn build_with<'a, Message: Clone + 'static>(
    node: Node<'_, Message>,
    theme: &FrTheme,
    forced: Option<Color>,
) -> El<'a, Message> {
    match node {
        Node::Column(flex) => {
            let children = flex
                .children
                .into_iter()
                .map(|child| build_with(child, theme, forced))
                .collect::<Vec<_>>();
            let column = Column::with_children(children)
                .spacing(f32::from(flex.spacing))
                .padding(padding(flex.padding))
                .width(length(flex.width))
                .height(length(flex.height))
                .align_x(horizontal(flex.align_x));
            surface(column.into(), flex.surface, theme)
        }
        Node::Row(flex) => {
            let children = flex
                .children
                .into_iter()
                .map(|child| build_with(child, theme, forced))
                .collect::<Vec<_>>();
            let row = Row::with_children(children)
                .spacing(f32::from(flex.spacing))
                .padding(padding(flex.padding))
                .width(length(flex.width))
                .height(length(flex.height))
                .align_y(vertical(flex.align_y));
            surface(row.into(), flex.surface, theme)
        }
        Node::Text(content) => {
            let size = content.style.size;
            let tone = content.style.tone;
            let weight = content.style.weight;
            let align = content.style.align;
            let wrap = content.style.wrap;
            let width = content.style.width;
            let mut widget = text(content.content.into_owned())
                .size(f32::from(size))
                .color(if tone == Tone::Text {
                    forced.unwrap_or_else(|| color(theme.text))
                } else {
                    color(theme.resolve(tone))
                })
                .width(length(width))
                .align_x(horizontal(align))
                .wrapping(if wrap {
                    widget::text::Wrapping::Word
                } else {
                    widget::text::Wrapping::None
                });
            if weight == Weight::Bold {
                widget = widget.font(iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::DEFAULT
                });
            }
            widget.into()
        }
        Node::Button(content) => {
            let (bg, fg): (Option<Color>, Color) = match content.kind {
                ButtonKind::Primary => (Some(color(theme.accent)), color(theme.on_accent)),
                ButtonKind::Danger => (
                    Some(color(theme.danger)),
                    color(theme.contrast_on(Tone::Danger)),
                ),
                ButtonKind::Secondary => (Some(color(theme.raised)), color(theme.text)),
                ButtonKind::Ghost => (
                    None,
                    forced.unwrap_or_else(|| color(theme.text)),
                ),
            };
            let border_color = color(theme.border);
            let accent = color(theme.accent);
            let kind = content.kind;
            let mut widget = button(build_with(*content.content, theme, Some(fg)))
                .padding(padding(content.padding))
                .width(length(content.width))
                .height(length(content.height))
                .style(move |_, status| {
                    let hovered = matches!(
                        status,
                        button::Status::Hovered | button::Status::Pressed
                    );
                    let background = match kind {
                        ButtonKind::Primary => Some(accent),
                        ButtonKind::Danger => Some(bg.unwrap_or(accent)),
                        ButtonKind::Secondary => Some(if hovered {
                            lighten(bg.unwrap_or(accent), 0.08)
                        } else {
                            bg.unwrap_or(accent)
                        }),
                        ButtonKind::Ghost => hovered.then_some(bg.unwrap_or(accent)),
                    };
                    button::Style {
                        background: background.map(iced::Background::Color),
                        text_color: fg,
                        border: Border {
                            color: border_color,
                            width: 0.0,
                            radius: 6.0.into(),
                        },
                        ..button::Style::default()
                    }
                });
            if let Some(message) = content.on_press {
                widget = widget.on_press(Event::User(message));
            }
            widget.into()
        }
        Node::TextInput(input) => {
            let value = input.value.into_owned();
            let placeholder = input.placeholder.into_owned();
            let width = length(input.width);
            let pad = padding(input.padding);
            let secure = input.secure;
            let on_submit = input.on_submit;
            let on_input = input.on_input;
            let raised = color(theme.raised);
            let border_color = color(theme.border);
            let text_color = color(theme.text);
            let placeholder_color = color(theme.muted);
            let selection = color(theme.accent);
            let mut widget = text_input(&placeholder, &value)
                .padding(pad)
                .width(width)
                .secure(secure)
                .style(move |_, _| text_input::Style {
                    background: raised.into(),
                    border: Border {
                        color: border_color,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    icon: placeholder_color,
                    placeholder: placeholder_color,
                    value: text_color,
                    selection,
                });
            if let Some(callback) = on_input {
                widget = widget.on_input(move |value| Event::User(callback.call(value)));
            }
            if let Some(message) = on_submit {
                widget = widget.on_submit(Event::User(message));
            }
            widget.into()
        }
        Node::Slider(content) => {
            let width = length(content.width);
            if let Some(callback) = content.on_change {
                let mut widget = slider(content.min..=content.max, content.value, move |value| {
                    Event::User(callback.call(value))
                })
                .width(width);
                if let Some(step) = content.step {
                    widget = widget.step(step);
                }
                if let Some(message) = content.on_release {
                    widget = widget.on_release(Event::User(message));
                }
                widget.into()
            } else {
                let fraction = content.fraction() as f32;
                progress_bar(0.0..=1.0, fraction).length(width).into()
            }
        }
        Node::List(list) => {
            let selected = list.selected;
            let activate = list.on_activate.clone();
            let select = list.on_select.clone();
            let width = length(list.width);
            let height = length(list.height);
            let accent = color(theme.accent);
            let raised = color(theme.raised);
            let text_color = color(theme.text);
            let muted = color(theme.muted);
            let on_accent = color(theme.on_accent);
            let mut column = Column::new().spacing(2.0);
            for (index, item) in list.items.into_iter().enumerate() {
                let is_selected = selected == Some(index);
                let is_accent = item.accent;
                let bg = if is_accent {
                    Some(accent)
                } else if is_selected {
                    Some(raised)
                } else {
                    None
                };
                let fg = if is_accent { on_accent } else { text_color };
                let callback = activate.clone().or_else(|| select.clone());
                let row = build_list_item(item, fg, muted);
                let mut widget = button(row)
                    .width(ILength::Fill)
                    .padding([5, 6])
                    .style(move |_, status| {
                        let hovered = matches!(
                            status,
                            button::Status::Hovered | button::Status::Pressed
                        );
                        button::Style {
                            background: bg
                                .or_else(|| hovered.then_some(lighten(raised, 0.06)))
                                .map(iced::Background::Color),
                            text_color: fg,
                            border: Border {
                                radius: 4.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    });
                if let Some(callback) = &callback {
                    widget = widget.on_press(Event::User(callback.call(index)));
                }
                column = column.push(widget);
            }
            let scroller = scrollable(column).width(width).height(height);
            scroller.into()
        }
        Node::Scroll(scroll) => {
            let inner = build(*scroll.child, theme);
            let inner = if scroll.padding.is_zero() {
                inner
            } else {
                container(inner).padding(padding(scroll.padding)).into()
            };
            scrollable(inner)
                .width(length(scroll.width))
                .height(length(scroll.height))
                .into()
        }
        Node::Image(content) => {
            let width = length(content.width);
            let height = length(content.height);
            let radius = f32::from(content.radius);
            if let Some(handle) = content.data.as_ref().and_then(handle_for) {
                image(handle)
                    .width(width)
                    .height(height)
                    .content_fit(match content.fit {
                        ImageFit::Cover => ContentFit::Cover,
                        ImageFit::Contain => ContentFit::Contain,
                        ImageFit::Fill => ContentFit::Fill,
                    })
                    .border_radius(radius)
                    .into()
            } else {
                let raised = color(theme.raised);
                let muted_color = color(theme.muted);
                container(text(content.alt.into_owned()).size(20).color(muted_color))
                    .width(width)
                    .height(height)
                    .center(width)
                    .style(move |_| container::Style {
                        background: Some(raised.into()),
                        border: Border {
                            radius: radius.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    })
                    .into()
            }
        }
        Node::Qr(content) => {
            let size = f32::from(content.size);
            match qr_handle(content.data.as_ref(), content.size, content.quiet_zone) {
                Some(handle) => image(handle)
                    .width(ILength::Fixed(size))
                    .height(ILength::Fixed(size))
                    .content_fit(ContentFit::Contain)
                    .into(),
                None => text("QR").color(color(theme.muted)).into(),
            }
        }
        Node::Progress(content) => progress_bar(0.0..=1.0, content.value as f32)
            .length(length(content.width))
            .girth(6.0)
            .into(),
        Node::Space(content) => space()
            .width(length(content.width))
            .height(length(content.height))
            .into(),
        Node::Divider(content) => {
            let line = color(theme.resolve(content.tone));
            let widget = if content.horizontal {
                rule::horizontal(1.0)
            } else {
                rule::vertical(1.0)
            };
            widget
                .style(move |_| rule::Style {
                    color: line,
                    radius: 0.0.into(),
                    fill_mode: rule::FillMode::Full,
                    snap: true,
                })
                .into()
        }
    }
}

fn build_list_item<'a, Message: 'static>(
    item: ListItem<'_>,
    fg: Color,
    muted: Color,
) -> Element<'a, Event<Message>> {
    let leading = item.leading.into_owned();
    let title = item.title.into_owned();
    let subtitle = item.subtitle.map(|text| text.into_owned());
    let trailing = item.trailing.map(|text| text.into_owned());
    let left: Element<'a, Event<Message>> = if leading.is_empty() {
        text(String::new()).into()
    } else {
        text(leading).size(13.0).color(muted).into()
    };
    let mut body = Column::new().spacing(1.0).push(
        text(title)
            .size(14.0)
            .color(fg)
            .wrapping(widget::text::Wrapping::Word),
    );
    if let Some(subtitle) = subtitle {
        body = body.push(text(subtitle).size(11.0).color(muted));
    }
    let mut row = Row::new()
        .spacing(8.0)
        .align_y(Alignment::Center)
        .push(left)
        .push(body.width(ILength::Fill));
    if let Some(trailing) = trailing {
        row = row.push(text(trailing).size(11.0).color(muted));
    }
    row.into()
}

fn surface<'a, Message: 'static>(
    element: Element<'a, Event<Message>>,
    surface: Surface,
    theme: &FrTheme,
) -> Element<'a, Event<Message>> {
    if surface.fill.is_none() && surface.border.is_none() {
        return element;
    }
    let background = surface.fill.map(|tone| color(theme.resolve(tone)));
    let border = surface.border.map(|tone| color(theme.resolve(tone)));
    let radius = f32::from(surface.radius);
    container(element)
        .style(move |_| container::Style {
            background: background.map(iced::Background::Color),
            border: Border {
                color: border.unwrap_or(Color::TRANSPARENT),
                width: if border.is_some() { 1.0 } else { 0.0 },
                radius: radius.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn handle_for(data: &ImageData) -> Option<image::Handle> {
    let id = data.id();
    if let Some(handle) = IMAGES.with(|images| images.borrow().get(&id).cloned()) {
        return Some(handle);
    }
    let handle = match data {
        ImageData::Encoded { bytes, .. } => image::Handle::from_bytes(bytes.as_ref().clone()),
        ImageData::Rgba {
            width,
            height,
            pixels,
            ..
        } => image::Handle::from_rgba(*width, *height, pixels.as_ref().clone()),
    };
    IMAGES.with(|images| {
        images.borrow_mut().insert(id, handle.clone());
    });
    Some(handle)
}

fn qr_handle(data: &str, size: u16, quiet_zone: u16) -> Option<image::Handle> {
    let key = hash_key(data, size, quiet_zone);
    if let Some(handle) = QRS.with(|qrs| qrs.borrow().get(&key).cloned()) {
        return Some(handle);
    }
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    let modules = code.width();
    let side = (modules + usize::from(quiet_zone) * 2).max(1);
    let scale = (u32::from(size) / side as u32).max(1);
    let pixels = {
        let mut pixels = vec![255u8; (side * side) as usize * 4];
        let colors = code.to_colors();
        for y in 0..modules {
            for x in 0..modules {
                if colors[y * modules + x] == qrcode::Color::Dark {
                    let index = (y * side + x) * 4;
                    pixels[index..index + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        let mut scaled = Vec::with_capacity((side * side) as usize * 16 * 4);
        for y in 0..side {
            for _ in 0..scale {
                for x in 0..side {
                    let index = (y * side + x) * 4;
                    for _ in 0..scale {
                        scaled.extend_from_slice(&pixels[index..index + 4]);
                    }
                }
            }
        }
        scaled
    };
    let side = (side as u32) * scale;
    let handle = image::Handle::from_rgba(side, side, pixels);
    QRS.with(|qrs| {
        qrs.borrow_mut().insert(key, handle.clone());
    });
    Some(handle)
}

fn hash_key(data: &str, size: u16, quiet_zone: u16) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    size.hash(&mut hasher);
    quiet_zone.hash(&mut hasher);
    hasher.finish()
}

fn lighten(color: Color, amount: f32) -> Color {
    Color {
        r: (color.r + amount).min(1.0),
        g: (color.g + amount).min(1.0),
        b: (color.b + amount).min(1.0),
        a: color.a,
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::Modifiers as IcedModifiers;

    #[test]
    fn shortcuts_keep_their_modifiers() {
        let event = map_key_fields(
            iced::keyboard::Key::Named(Named::Space),
            IcedModifiers::CTRL,
            true,
            false,
            None,
        );
        assert_eq!(event.key, Key::Space);
        assert!(event.modifiers.control);
        assert!(event.modifiers.is_shortcut());
        assert!(event.is_press());
    }

    #[test]
    fn named_keys_map_to_the_shared_set() {
        let event = map_key_fields(
            iced::keyboard::Key::Named(Named::ArrowLeft),
            IcedModifiers::NONE,
            true,
            false,
            None,
        );
        assert_eq!(event.key, Key::Left);
        let event = map_key_fields(
            iced::keyboard::Key::Character("j".into()),
            IcedModifiers::NONE,
            true,
            false,
            Some("j".into()),
        );
        assert_eq!(event.key, Key::Character("j".into()));
        assert_eq!(event.text(), Some("j"));
    }

    #[test]
    fn the_theme_palette_follows_the_ferriso_theme() {
        let ferriso_theme = FrTheme::light();
        let theme = iced_theme(&ferriso_theme);
        assert_eq!(theme.palette().background, color(ferriso_theme.background));
        assert_eq!(theme.palette().primary, color(ferriso_theme.accent));
    }

    #[test]
    fn qr_pixels_are_generated_and_cached() {
        let first = qr_handle("https://example.com/login?key=1", 120, 2).unwrap();
        let second = qr_handle("https://example.com/login?key=1", 120, 2).unwrap();
        assert_eq!(first, second);
    }
}
