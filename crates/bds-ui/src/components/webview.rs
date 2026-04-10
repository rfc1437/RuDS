use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use iced::event;
use iced::advanced::layout::{self, Node};
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Clipboard, Layout, Shell, Widget};
use iced::mouse;
use iced::{Element, Event, Length, Rectangle, Size, Task, window};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::{Rect, WebView, WebViewBuilder};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static STAGED: RefCell<HashMap<u64, WebView>> = RefCell::new(HashMap::new());
}

pub enum Content {
    Url(String),
    Html(String),
}

impl Default for Content {
    fn default() -> Self {
        Self::Url(String::new())
    }
}

#[derive(Default)]
pub struct WebViewConfig {
    content: Content,
    transparent: bool,
    devtools: bool,
}

impl WebViewConfig {
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.content = Content::Url(url.into());
        self
    }

    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.content = Content::Html(html.into());
        self
    }

    pub fn transparent(mut self, transparent: bool) -> Self {
        self.transparent = transparent;
        self
    }

    pub fn devtools(mut self, devtools: bool) -> Self {
        self.devtools = devtools;
        self
    }
}

struct SharedState {
    webview: Option<WebView>,
    last_bounds: Option<Rectangle>,
}

#[derive(Clone)]
pub(crate) struct BoundsSender(Rc<RefCell<SharedState>>);

impl BoundsSender {
    pub(crate) fn apply(&self, bounds: Rectangle) {
        let mut state = self.0.borrow_mut();
        state.last_bounds = Some(bounds);
        if let Some(webview) = &state.webview {
            let rect = Rect {
                position: LogicalPosition::new(bounds.x as f64, bounds.y as f64).into(),
                size: LogicalSize::new(bounds.width as f64, bounds.height as f64).into(),
            };
            let _ = webview.set_bounds(rect);
        }
    }

    pub(crate) fn refocus_parent(&self) {
        let state = self.0.borrow();
        if let Some(webview) = &state.webview {
            let _ = webview.focus_parent();
        }
    }
}

pub struct WebViewController {
    id: u64,
    shared: Rc<RefCell<SharedState>>,
    config: WebViewConfig,
}

impl WebViewController {
    pub fn new(config: WebViewConfig) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            shared: Rc::new(RefCell::new(SharedState {
                webview: None,
                last_bounds: None,
            })),
            config,
        }
    }

    pub(crate) fn bounds_sender(&self) -> BoundsSender {
        BoundsSender(Rc::clone(&self.shared))
    }

    pub fn create_task<M: Send + 'static>(
        &mut self,
        window_id: window::Id,
        on_result: fn(Result<(), String>) -> M,
    ) -> Task<M> {
        let id = self.id;
        let content = std::mem::take(&mut self.config.content);
        let transparent = self.config.transparent;
        let devtools = self.config.devtools;

        window::run_with_handle(window_id, move |handle| {
            build_webview(id, handle, content, transparent, devtools)
        })
        .map(on_result)
    }

    pub fn take_staged(&mut self) {
        let webview = STAGED.with(|cell| cell.borrow_mut().remove(&self.id));
        let mut state = self.shared.borrow_mut();
        state.webview = webview;

        if let (Some(webview), Some(bounds)) = (&state.webview, state.last_bounds) {
            let rect = Rect {
                position: LogicalPosition::new(bounds.x as f64, bounds.y as f64).into(),
                size: LogicalSize::new(bounds.width as f64, bounds.height as f64).into(),
            };
            let _ = webview.set_bounds(rect);
        }
    }

    pub fn set_visible(&self, visible: bool) {
        let state = self.shared.borrow();
        if let Some(webview) = &state.webview {
            let _ = webview.set_visible(visible);
        }
    }

    pub fn navigate(&self, url: &str) {
        let state = self.shared.borrow();
        if let Some(webview) = &state.webview {
            let _ = webview.load_url(url);
        }
    }

    pub fn destroy(&mut self) {
        self.shared.borrow_mut().webview = None;
    }

    pub fn is_active(&self) -> bool {
        self.shared.borrow().webview.is_some()
    }
}

fn build_webview(
    id: u64,
    handle: window::raw_window_handle::WindowHandle<'_>,
    content: Content,
    transparent: bool,
    devtools: bool,
) -> Result<(), String> {
    let mut builder = WebViewBuilder::new()
        .with_transparent(transparent)
        .with_devtools(devtools)
        .with_focused(false);

    builder = match content {
        Content::Html(html) => builder.with_html(html),
        Content::Url(url) => builder.with_url(url),
    };

    let webview = builder
        .build_as_child(&handle)
        .map_err(|error| error.to_string())?;

    STAGED.with(|cell| {
        cell.borrow_mut().insert(id, webview);
    });

    Ok(())
}

#[derive(Default)]
struct PlaceholderState {
    last_bounds: Option<Rectangle>,
}

pub struct WebViewPlaceholder<Message> {
    width: Length,
    height: Length,
    bounds_tx: Option<BoundsSender>,
    _message: std::marker::PhantomData<Message>,
}

impl<Message> WebViewPlaceholder<Message> {
    pub fn new() -> Self {
        Self {
            width: Length::Fill,
            height: Length::Fill,
            bounds_tx: None,
            _message: std::marker::PhantomData,
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

    pub(crate) fn bounds_sender(mut self, sender: BoundsSender) -> Self {
        self.bounds_tx = Some(sender);
        self
    }
}

impl<Message> Default for WebViewPlaceholder<Message> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for WebViewPlaceholder<Message>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<PlaceholderState>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(PlaceholderState::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&self, _tree: &mut Tree, _renderer: &Renderer, limits: &layout::Limits) -> Node {
        Node::new(limits.resolve(self.width, self.height, Size::ZERO))
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let Some(tx) = &self.bounds_tx else {
            return event::Status::Ignored;
        };

        let state = tree.state.downcast_mut::<PlaceholderState>();
        let bounds = layout.bounds();

        if state.last_bounds.as_ref() != Some(&bounds) {
            state.last_bounds = Some(bounds);
            tx.apply(bounds);
        }

        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && !cursor.is_over(bounds)
        {
            tx.refocus_parent();
        }

        event::Status::Ignored
    }
}

impl<'a, Message, Theme, Renderer> From<WebViewPlaceholder<Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(placeholder: WebViewPlaceholder<Message>) -> Self {
        Self::new(placeholder)
    }
}

pub fn webview<Message>(controller: &WebViewController) -> WebViewPlaceholder<Message> {
    WebViewPlaceholder::new().bounds_sender(controller.bounds_sender())
}