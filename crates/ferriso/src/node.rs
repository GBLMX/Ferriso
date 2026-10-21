//! The view IR: a semantic description of a screen.
//!
//! Nodes carry data and callbacks, never renderer types. A backend walks the same tree and
//! decides how to draw it, where to put focus, and how a terminal cell differs from a pixel.
//! The set is intentionally small: every node has an obvious mapping to both ratatui and iced,
//! and an application that needs something else should compose these rather than teach one
//! renderer a private widget.

use std::{borrow::Cow, fmt, sync::Arc};

use crate::theme::Tone;

/// Stable identity for stateful widgets (text cursor, scroll offset, list viewport). Without
/// one, renderers derive a path-based identity, which is correct for a static layout and
/// merely loses local state when the layout is rebuilt in a different order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId(pub u64);

impl WidgetId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

/// How much space a child asks for on its parent's main axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Length {
    /// The widget's intrinsic size.
    #[default]
    Shrink,
    /// Share the remaining space equally with other `Fill` siblings.
    Fill,
    /// Exactly this many logical units (cells in a terminal, pixels in a window).
    Fixed(u16),
    /// Share the remaining space by weight.
    FillPortion(u16),
}

impl From<u16> for Length {
    fn from(value: u16) -> Self {
        Self::Fixed(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Padding {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Padding {
    pub const ZERO: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn symmetric(horizontal: u16, vertical: u16) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    pub const fn new(top: u16, right: u16, bottom: u16, left: u16) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn horizontal(&self) -> u16 {
        self.left.saturating_add(self.right)
    }

    pub const fn vertical(&self) -> u16 {
        self.top.saturating_add(self.bottom)
    }

    pub const fn is_zero(&self) -> bool {
        self.top == 0 && self.right == 0 && self.bottom == 0 && self.left == 0
    }
}

impl From<u16> for Padding {
    fn from(value: u16) -> Self {
        Self::all(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weight {
    #[default]
    Normal,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextStyle {
    /// A GUI hint. Terminals have one glyph size; they map this to emphasis only through
    /// [`Weight`], never by inventing a font scale.
    pub size: u16,
    pub tone: Tone,
    pub weight: Weight,
    pub align: Align,
    pub width: Length,
    pub wrap: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size: 14,
            tone: Tone::Text,
            weight: Weight::Normal,
            align: Align::Start,
            width: Length::Shrink,
            wrap: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    pub fill: Option<Tone>,
    pub border: Option<Tone>,
    /// Window renderers honour this; a terminal rounds it to square corners.
    pub radius: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

/// Owned event handler. Renderers return widgets built from a temporary view tree, so a
/// callback cannot borrow that tree; cloning the `Arc` is what lets an iced element outlive it.
pub struct Callback<Input, Message> {
    run: Arc<dyn Fn(Input) -> Message>,
}

impl<Input, Message> Clone for Callback<Input, Message> {
    fn clone(&self) -> Self {
        Self {
            run: Arc::clone(&self.run),
        }
    }
}

impl<Input, Message> Callback<Input, Message> {
    pub fn new(run: impl Fn(Input) -> Message + 'static) -> Self {
        Self { run: Arc::new(run) }
    }

    pub fn call(&self, input: Input) -> Message {
        (self.run)(input)
    }
}

impl<Input, Message> fmt::Debug for Callback<Input, Message> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Callback")
    }
}

pub enum Node<'a, Message> {
    Column(Flex<'a, Message>),
    Row(Flex<'a, Message>),
    Text(Text<'a>),
    Button(Button<'a, Message>),
    TextInput(TextInput<'a, Message>),
    Slider(Slider<Message>),
    List(List<'a, Message>),
    Scroll(Scroll<'a, Message>),
    Image(Image<'a>),
    Qr(Qr<'a>),
    Progress(Progress),
    Space(Space),
    Divider(Divider),
}

impl<'a, Message> fmt::Debug for Node<'a, Message> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Column(_) => "Column",
            Self::Row(_) => "Row",
            Self::Text(_) => "Text",
            Self::Button(_) => "Button",
            Self::TextInput(_) => "TextInput",
            Self::Slider(_) => "Slider",
            Self::List(_) => "List",
            Self::Scroll(_) => "Scroll",
            Self::Image(_) => "Image",
            Self::Qr(_) => "Qr",
            Self::Progress(_) => "Progress",
            Self::Space(_) => "Space",
            Self::Divider(_) => "Divider",
        };
        f.write_str(name)
    }
}

impl<'a, Message> From<Text<'a>> for Node<'a, Message> {
    fn from(value: Text<'a>) -> Self {
        Self::Text(value)
    }
}

impl<'a, Message> From<&'a str> for Node<'a, Message> {
    fn from(value: &'a str) -> Self {
        Self::Text(Text::new(value))
    }
}

impl<'a, Message> From<String> for Node<'a, Message> {
    fn from(value: String) -> Self {
        Self::Text(Text::new(value))
    }
}

impl<'a, Message> From<Button<'a, Message>> for Node<'a, Message> {
    fn from(value: Button<'a, Message>) -> Self {
        Self::Button(value)
    }
}

impl<'a, Message> From<TextInput<'a, Message>> for Node<'a, Message> {
    fn from(value: TextInput<'a, Message>) -> Self {
        Self::TextInput(value)
    }
}

impl<'a, Message> From<Slider<Message>> for Node<'a, Message> {
    fn from(value: Slider<Message>) -> Self {
        Self::Slider(value)
    }
}

impl<'a, Message> From<List<'a, Message>> for Node<'a, Message> {
    fn from(value: List<'a, Message>) -> Self {
        Self::List(value)
    }
}

impl<'a, Message> From<Scroll<'a, Message>> for Node<'a, Message> {
    fn from(value: Scroll<'a, Message>) -> Self {
        Self::Scroll(value)
    }
}

impl<'a, Message> From<Image<'a>> for Node<'a, Message> {
    fn from(value: Image<'a>) -> Self {
        Self::Image(value)
    }
}

impl<'a, Message> From<Qr<'a>> for Node<'a, Message> {
    fn from(value: Qr<'a>) -> Self {
        Self::Qr(value)
    }
}

impl<'a, Message> From<Progress> for Node<'a, Message> {
    fn from(value: Progress) -> Self {
        Self::Progress(value)
    }
}

impl<'a, Message> From<Space> for Node<'a, Message> {
    fn from(value: Space) -> Self {
        Self::Space(value)
    }
}

impl<'a, Message> From<Divider> for Node<'a, Message> {
    fn from(value: Divider) -> Self {
        Self::Divider(value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Vertical,
    Horizontal,
}

/// A stack. Build with [`column`] or [`row`]; the direction is baked into the node when it is
/// converted, so renderers never have to infer it from the children.
pub struct Flex<'a, Message> {
    pub direction: Direction,
    pub children: Vec<Node<'a, Message>>,
    pub spacing: u16,
    pub padding: Padding,
    pub width: Length,
    pub height: Length,
    pub align_x: Align,
    pub align_y: Align,
    pub surface: Surface,
}

impl<'a, Message> Default for Flex<'a, Message> {
    fn default() -> Self {
        Self {
            direction: Direction::Vertical,
            children: Vec::new(),
            spacing: 0,
            padding: Padding::ZERO,
            width: Length::Fill,
            height: Length::Shrink,
            align_x: Align::Start,
            align_y: Align::Start,
            surface: Surface::default(),
        }
    }
}

impl<'a, Message> Flex<'a, Message> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(mut self, child: impl Into<Node<'a, Message>>) -> Self {
        self.children.push(child.into());
        self
    }

    pub fn extend(mut self, children: impl IntoIterator<Item = Node<'a, Message>>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn align_x(mut self, align: Align) -> Self {
        self.align_x = align;
        self
    }

    pub fn align_y(mut self, align: Align) -> Self {
        self.align_y = align;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn fill(mut self, tone: Tone) -> Self {
        self.surface.fill = Some(tone);
        self
    }

    pub fn border(mut self, tone: Tone) -> Self {
        self.surface.border = Some(tone);
        self
    }

    pub fn radius(mut self, radius: u16) -> Self {
        self.surface.radius = radius;
        self
    }
}

/// Vertical stack.
pub fn column<'a, Message>() -> Flex<'a, Message> {
    Flex::new()
}

/// Horizontal stack.
pub fn row<'a, Message>() -> Flex<'a, Message> {
    let mut flex = Flex::new();
    flex.direction = Direction::Horizontal;
    flex
}

impl<'a, Message> From<Flex<'a, Message>> for Node<'a, Message> {
    fn from(value: Flex<'a, Message>) -> Self {
        match value.direction {
            Direction::Vertical => Self::Column(value),
            Direction::Horizontal => Self::Row(value),
        }
    }
}

pub struct Text<'a> {
    pub content: Cow<'a, str>,
    pub style: TextStyle,
}

impl<'a> Text<'a> {
    pub fn new(content: impl Into<Cow<'a, str>>) -> Self {
        Self {
            content: content.into(),
            style: TextStyle::default(),
        }
    }

    pub fn size(mut self, size: u16) -> Self {
        self.style.size = size;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.style.tone = tone;
        self
    }

    pub fn bold(mut self) -> Self {
        self.style.weight = Weight::Bold;
        self
    }

    pub fn weight(mut self, weight: Weight) -> Self {
        self.style.weight = weight;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.style.align = align;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.style.width = width.into();
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.style.wrap = wrap;
        self
    }
}

/// Convenience for the common one-line text child.
pub fn text<'a>(content: impl Into<Cow<'a, str>>) -> Text<'a> {
    Text::new(content)
}

/// A labelled button that always sends `message` when pressed.
pub fn button<'a, Message>(label: impl Into<Cow<'a, str>>, message: Message) -> Button<'a, Message> {
    Button::label(label).on_press(message)
}

pub struct Button<'a, Message> {
    pub content: Box<Node<'a, Message>>,
    pub on_press: Option<Message>,
    pub padding: Padding,
    pub width: Length,
    pub height: Length,
    pub kind: ButtonKind,
    pub align_x: Align,
}

impl<'a, Message> Button<'a, Message> {
    pub fn new(content: impl Into<Node<'a, Message>>) -> Self {
        Self {
            content: Box::new(content.into()),
            on_press: None,
            padding: Padding::symmetric(10, 4),
            width: Length::Shrink,
            height: Length::Shrink,
            kind: ButtonKind::Secondary,
            align_x: Align::Start,
        }
    }

    pub fn label(label: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Text::new(label))
    }

    pub fn on_press(mut self, message: Message) -> Self {
        self.on_press = Some(message);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        if disabled {
            self.on_press = None;
        }
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn align_x(mut self, align: Align) -> Self {
        self.align_x = align;
        self
    }
}

pub struct TextInput<'a, Message> {
    pub id: Option<WidgetId>,
    pub value: Cow<'a, str>,
    pub placeholder: Cow<'a, str>,
    pub on_input: Option<Callback<String, Message>>,
    pub on_submit: Option<Message>,
    pub width: Length,
    pub padding: Padding,
    pub secure: bool,
}

impl<'a, Message> TextInput<'a, Message> {
    pub fn new(value: impl Into<Cow<'a, str>>) -> Self {
        Self {
            id: None,
            value: value.into(),
            placeholder: Cow::Borrowed(""),
            on_input: None,
            on_submit: None,
            width: Length::Fill,
            padding: Padding::symmetric(8, 4),
            secure: false,
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<Cow<'a, str>>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn on_input(mut self, run: impl Fn(String) -> Message + 'static) -> Self {
        self.on_input = Some(Callback::new(run));
        self
    }

    pub fn on_submit(mut self, message: Message) -> Self {
        self.on_submit = Some(message);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }
}

pub struct Slider<Message> {
    pub id: Option<WidgetId>,
    pub min: f64,
    pub max: f64,
    pub value: f64,
    pub step: Option<f64>,
    pub on_change: Option<Callback<f64, Message>>,
    pub on_release: Option<Message>,
    pub width: Length,
}

impl<Message> Slider<Message> {
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self {
            id: None,
            min,
            max,
            value,
            step: None,
            on_change: None,
            on_release: None,
            width: Length::Fill,
        }
    }

    pub fn on_change(mut self, run: impl Fn(f64) -> Message + 'static) -> Self {
        self.on_change = Some(Callback::new(run));
        self
    }

    pub fn on_release(mut self, message: Message) -> Self {
        self.on_release = Some(message);
        self
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn fraction(&self) -> f64 {
        if self.max <= self.min {
            0.0
        } else {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }
}

pub struct ListItem<'a> {
    /// The playing marker, row number, or icon.
    pub leading: Cow<'a, str>,
    pub title: Cow<'a, str>,
    pub subtitle: Option<Cow<'a, str>>,
    pub trailing: Option<Cow<'a, str>>,
    /// The row is the active item (playing song, current result), not merely selected.
    pub accent: bool,
}

impl<'a> ListItem<'a> {
    pub fn new(title: impl Into<Cow<'a, str>>) -> Self {
        Self {
            leading: Cow::Borrowed(""),
            title: title.into(),
            subtitle: None,
            trailing: None,
            accent: false,
        }
    }

    pub fn leading(mut self, leading: impl Into<Cow<'a, str>>) -> Self {
        self.leading = leading.into();
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<Cow<'a, str>>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn trailing(mut self, trailing: impl Into<Cow<'a, str>>) -> Self {
        self.trailing = Some(trailing.into());
        self
    }

    pub fn accent(mut self, accent: bool) -> Self {
        self.accent = accent;
        self
    }
}

pub struct List<'a, Message> {
    pub id: Option<WidgetId>,
    pub items: Vec<ListItem<'a>>,
    pub selected: Option<usize>,
    pub on_select: Option<Callback<usize, Message>>,
    pub on_activate: Option<Callback<usize, Message>>,
    pub row_height: u16,
    pub width: Length,
    pub height: Length,
}

impl<'a, Message> List<'a, Message> {
    pub fn new(items: Vec<ListItem<'a>>) -> Self {
        Self {
            id: None,
            items,
            selected: None,
            on_select: None,
            on_activate: None,
            row_height: 24,
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_select(mut self, run: impl Fn(usize) -> Message + 'static) -> Self {
        self.on_select = Some(Callback::new(run));
        self
    }

    pub fn on_activate(mut self, run: impl Fn(usize) -> Message + 'static) -> Self {
        self.on_activate = Some(Callback::new(run));
        self
    }

    pub fn row_height(mut self, row_height: u16) -> Self {
        self.row_height = row_height;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }
}

pub struct Scroll<'a, Message> {
    pub id: Option<WidgetId>,
    pub child: Box<Node<'a, Message>>,
    pub width: Length,
    pub height: Length,
    pub padding: Padding,
    pub show_scrollbar: bool,
}

impl<'a, Message> Scroll<'a, Message> {
    pub fn new(child: impl Into<Node<'a, Message>>) -> Self {
        Self {
            id: None,
            child: Box::new(child.into()),
            width: Length::Fill,
            height: Length::Fill,
            padding: Padding::ZERO,
            show_scrollbar: true,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    pub fn scrollbar(mut self, show: bool) -> Self {
        self.show_scrollbar = show;
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
    Fill,
}

/// Image payload. The renderer owns decoding; `id` lets it cache the decoded handle across
/// frames even though the tree is rebuilt on every view call.
#[derive(Clone)]
pub enum ImageData {
    Encoded {
        id: u64,
        bytes: Arc<Vec<u8>>,
    },
    Rgba {
        id: u64,
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    },
}

impl ImageData {
    pub fn id(&self) -> u64 {
        match self {
            Self::Encoded { id, .. } | Self::Rgba { id, .. } => *id,
        }
    }
}

pub struct Image<'a> {
    pub data: Option<ImageData>,
    pub alt: Cow<'a, str>,
    pub width: Length,
    pub height: Length,
    pub fit: ImageFit,
    pub radius: u16,
}

impl<'a> Image<'a> {
    pub fn new(data: Option<ImageData>) -> Self {
        Self {
            data,
            alt: Cow::Borrowed(""),
            width: Length::Fill,
            height: Length::Fill,
            fit: ImageFit::Cover,
            radius: 0,
        }
    }

    pub fn alt(mut self, alt: impl Into<Cow<'a, str>>) -> Self {
        self.alt = alt.into();
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn radius(mut self, radius: u16) -> Self {
        self.radius = radius;
        self
    }
}

/// A QR payload. Encoding is a renderer concern; terminal backends draw modules as cells.
pub struct Qr<'a> {
    pub data: Cow<'a, str>,
    /// Desired side length in logical units.
    pub size: u16,
    pub quiet_zone: u16,
}

impl<'a> Qr<'a> {
    pub fn new(data: impl Into<Cow<'a, str>>) -> Self {
        Self {
            data: data.into(),
            size: 240,
            quiet_zone: 2,
        }
    }

    pub fn size(mut self, size: u16) -> Self {
        self.size = size;
        self
    }

    pub fn quiet_zone(mut self, quiet_zone: u16) -> Self {
        self.quiet_zone = quiet_zone;
        self
    }
}

pub struct Progress {
    pub value: f64,
    pub width: Length,
    pub tone: Tone,
}

impl Progress {
    pub fn new(value: f64) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            width: Length::Fill,
            tone: Tone::Accent,
        }
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

pub struct Space {
    pub width: Length,
    pub height: Length,
}

/// A flexible gap: takes its share of a row or column without drawing anything.
pub fn space() -> Space {
    Space::new(Length::Fill, Length::Fill)
}

impl Space {
    pub fn new(width: impl Into<Length>, height: impl Into<Length>) -> Self {
        Self {
            width: width.into(),
            height: height.into(),
        }
    }

    pub fn fixed(value: u16) -> Self {
        Self::new(Length::Fixed(value), Length::Fixed(value))
    }
}

pub struct Divider {
    pub horizontal: bool,
    pub tone: Tone,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self {
            horizontal: true,
            tone: Tone::Border,
        }
    }

    pub fn vertical() -> Self {
        Self {
            horizontal: false,
            tone: Tone::Border,
        }
    }
}
