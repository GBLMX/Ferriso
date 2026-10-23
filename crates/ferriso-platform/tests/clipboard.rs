#![cfg(feature = "clipboard")]

use std::sync::Arc;

use ferriso_platform::{Clipboard, MemoryClipboard};

#[test]
fn applications_can_inject_a_clipboard_through_the_trait() {
    let memory = Arc::new(MemoryClipboard::new());
    let clipboard: Arc<dyn Clipboard> = memory.clone();

    clipboard.set_text("selected text").expect("write");
    assert_eq!(
        clipboard.get_text().expect("read"),
        Some("selected text".into())
    );
    assert_eq!(memory.snapshot(), Some("selected text".into()));
}
